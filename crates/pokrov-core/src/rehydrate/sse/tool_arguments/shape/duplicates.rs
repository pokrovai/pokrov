//! Bounded per-event plan that detaches argument values for restoration.

use serde_json::Value;

use super::super::{ArgumentKey, ArgumentStreams, MAX_EVENT_ENTRIES};
use super::{
    DetachedArguments, DetachedDelta, DetachedToolCalls, EventPlan, PlannedChoice, PlannedTool,
    ProcessPlan,
};

#[derive(Clone, Copy)]
enum ArgsKind {
    Absent,
    String,
    Other,
}

struct RawTool {
    position: usize,
    index: Option<u64>,
    args: ArgsKind,
}
enum RawTools {
    Absent,
    Entries(Vec<RawTool>),
    Malformed,
}

impl RawTools {
    fn is_tool_call_bearing(&self) -> bool {
        match self {
            Self::Absent => false,
            Self::Malformed => true,
            Self::Entries(tools) => !tools.is_empty(),
        }
    }
}
struct RawChoice {
    position: usize,
    index: Option<u64>,
    finish: bool,
    tools: RawTools,
}

/// An error-bearing frame is processable only when it also carries a real
/// Chat choice: numeric `index` plus a delta object or a non-null string
/// `finish_reason`. A nonempty array of empty objects carries no usable chunk.
fn is_usable_error_choice(choice: &Value) -> bool {
    let Some(object) = choice.as_object() else {
        return false;
    };
    if object.get("index").and_then(Value::as_u64).is_none() {
        return false;
    }
    object.get("delta").is_some_and(Value::is_object)
        || object.get("finish_reason").and_then(Value::as_str).is_some()
}

pub(crate) fn plan_event(json: &mut Value, streams: &ArgumentStreams) -> EventPlan {
    if json.get("choices").is_none() {
        return if json.get("error").is_some() {
            EventPlan::Opaque
        } else {
            EventPlan::Process(ProcessPlan::empty())
        };
    }
    if !json.get("choices").is_some_and(Value::is_array) {
        return EventPlan::Opaque;
    }
    if json.get("error").is_some()
        && !json
            .get("choices")
            .and_then(Value::as_array)
            .is_some_and(|choices| choices.iter().any(is_usable_error_choice))
    {
        return EventPlan::Opaque;
    }

    let mut count = json.get("choices").and_then(Value::as_array).map_or(0, Vec::len);
    if count > MAX_EVENT_ENTRIES {
        return EventPlan::Opaque;
    }
    let raw = {
        let choices = json.get("choices").and_then(Value::as_array).expect("checked array");
        let mut raw = Vec::new();
        for (position, choice) in choices.iter().enumerate() {
            let Some(object) = choice.as_object() else {
                return EventPlan::Opaque;
            };
            let index = object.get("index").and_then(Value::as_u64);
            let finish = object.get("finish_reason").is_some_and(|reason| !reason.is_null());
            let mut tools = RawTools::Absent;
            match object.get("delta") {
                None => {}
                Some(delta) => {
                    let Some(delta) = delta.as_object() else {
                        raw.push(RawChoice { position, index, finish, tools: RawTools::Malformed });
                        continue;
                    };
                    if let Some(tool_calls) = delta.get("tool_calls") {
                        let Some(entries) = tool_calls.as_array() else {
                            raw.push(RawChoice {
                                position,
                                index,
                                finish,
                                tools: RawTools::Malformed,
                            });
                            continue;
                        };
                        count = count.saturating_add(entries.len());
                        if count > MAX_EVENT_ENTRIES {
                            return EventPlan::Opaque;
                        }
                        let mut parsed = Vec::new();
                        let mut malformed = false;
                        for (tool_position, entry) in entries.iter().enumerate() {
                            let Some(tool) = entry.as_object() else {
                                malformed = true;
                                break;
                            };
                            let tool_index = tool.get("index").and_then(Value::as_u64);
                            let mut args = ArgsKind::Absent;
                            if let Some(function) = tool.get("function") {
                                let Some(function) = function.as_object() else {
                                    malformed = true;
                                    break;
                                };
                                match function.get("arguments") {
                                    None => {}
                                    Some(Value::String(_)) => args = ArgsKind::String,
                                    Some(_) => args = ArgsKind::Other,
                                }
                            }
                            parsed.push(RawTool {
                                position: tool_position,
                                index: tool_index,
                                args,
                            });
                        }
                        tools =
                            if malformed { RawTools::Malformed } else { RawTools::Entries(parsed) };
                    }
                }
            }
            raw.push(RawChoice { position, index, finish, tools });
        }
        raw
    };

    let mut duplicate_choices: Vec<u64> = Vec::new();
    for (idx, choice) in raw.iter().enumerate() {
        let Some(index) = choice.index else { continue };
        if raw[..idx].iter().any(|earlier| earlier.index == Some(index))
            && !duplicate_choices.contains(&index)
        {
            duplicate_choices.push(index);
        }
    }
    let global_gap = raw.iter().any(|choice| {
        choice.index.is_none() && (choice.finish || choice.tools.is_tool_call_bearing())
    });

    let mut flush_choices: Vec<u64> = Vec::new();
    let mut flush_pairs: Vec<ArgumentKey> = Vec::new();
    let mut quarantined = vec![false; raw.len()];
    let mut duplicate_tools: Vec<Vec<ArgumentKey>> = vec![Vec::new(); raw.len()];
    if !global_gap {
        for (idx, choice) in raw.iter().enumerate() {
            let Some(index) = choice.index else { continue };
            if duplicate_choices.contains(&index) {
                quarantined[idx] = true;
                push_unique(&mut flush_choices, index);
                continue;
            }
            if streams.is_choice_fenced(index) {
                quarantined[idx] = true;
                continue;
            }
            match &choice.tools {
                RawTools::Malformed => {
                    quarantined[idx] = true;
                    push_unique(&mut flush_choices, index);
                }
                RawTools::Absent => {}
                RawTools::Entries(tools) => {
                    if tools.iter().any(|tool| tool.index.is_none()) {
                        quarantined[idx] = true;
                        push_unique(&mut flush_choices, index);
                        continue;
                    }
                    for (tool_idx, tool) in tools.iter().enumerate() {
                        let key =
                            ArgumentKey { choice: index, tool: tool.index.expect("validated") };
                        if tools[..tool_idx].iter().any(|earlier| earlier.index == Some(key.tool))
                            && !duplicate_tools[idx].contains(&key)
                        {
                            duplicate_tools[idx].push(key);
                        }
                        if matches!(tool.args, ArgsKind::Other) {
                            push_key(&mut flush_pairs, key);
                        }
                    }
                    for key in &duplicate_tools[idx] {
                        push_key(&mut flush_pairs, *key);
                    }
                }
            }
        }
    }

    let mut plan = ProcessPlan {
        global_gap,
        choices: Vec::with_capacity(raw.len()),
        detached: Vec::new(),
        detached_tool_calls: Vec::new(),
        detached_deltas: Vec::new(),
        flush_choices,
        flush_pairs,
    };
    let choices = json.get_mut("choices").and_then(Value::as_array_mut).expect("checked array");
    for (idx, raw_choice) in raw.into_iter().enumerate() {
        let position = raw_choice.position;
        let choice = &mut choices[position];
        let choice_quarantined = global_gap || quarantined[idx];
        let mut planned_tools = Vec::new();
        match raw_choice.tools {
            RawTools::Malformed => {
                if let Some(value) = take_tool_calls(choice) {
                    plan.detached_tool_calls.push(DetachedToolCalls { choice: position, value });
                } else if let Some(value) = take_delta(choice) {
                    plan.detached_deltas.push(DetachedDelta { choice: position, value });
                }
            }
            RawTools::Absent => {}
            RawTools::Entries(tools) => {
                for tool in tools {
                    let Some(original) = take_arguments(choice, tool.position) else { continue };
                    let key = raw_choice
                        .index
                        .zip(tool.index)
                        .map(|(choice, tool)| ArgumentKey { choice, tool });
                    let pair_closed = key.is_some_and(|key| {
                        duplicate_tools[idx].contains(&key) || streams.is_pair_closed(key)
                    });
                    let process =
                        !choice_quarantined && !pair_closed && matches!(original, Value::String(_));
                    let writeback = plan.detached.len();
                    plan.detached.push(DetachedArguments {
                        choice: position,
                        tool: tool.position,
                        value: original,
                        process,
                    });
                    if let (true, Some(key)) = (process, key) {
                        planned_tools.push(PlannedTool { key, writeback });
                    }
                }
            }
        }
        plan.choices.push(PlannedChoice {
            index: raw_choice.index,
            finish: raw_choice.finish,
            quarantined: choice_quarantined,
            tools: planned_tools,
        });
    }
    EventPlan::Process(plan)
}

fn push_unique(values: &mut Vec<u64>, value: u64) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_key(values: &mut Vec<ArgumentKey>, value: ArgumentKey) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn take_arguments(choice: &mut Value, tool: usize) -> Option<Value> {
    let function = choice.pointer_mut(&format!("/delta/tool_calls/{tool}/function"))?;
    let slot = function.as_object_mut()?.get_mut("arguments")?;
    Some(std::mem::replace(slot, Value::Null))
}

fn take_tool_calls(choice: &mut Value) -> Option<Value> {
    let slot = choice.get_mut("delta")?.as_object_mut()?.get_mut("tool_calls")?;
    Some(std::mem::replace(slot, Value::Null))
}

fn take_delta(choice: &mut Value) -> Option<Value> {
    let slot = choice.get_mut("delta")?;
    Some(std::mem::replace(slot, Value::Null))
}
