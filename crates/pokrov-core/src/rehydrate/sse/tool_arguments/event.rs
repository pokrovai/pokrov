//! Per-event Chat Completions pipeline: line partitioning, detached-argument
//! planning, restoration, and synthetic suffix placement.

use super::shape::{self, EventPlan, ProcessPlan};
use super::streams::{flush_all_and_close, EnvelopeCandidate};
use super::{has_ambiguous_chat_tool_members, ArgumentStreams};
use crate::rehydrate::{count_unrestored_markers, EventRehydrator, RehydrateReport};
use serde_json::Value;
use std::borrow::Cow;

/// Entry point for `EventRehydrator::rehydrate_event` in Chat mode.
pub(crate) fn rehydrate_chat_event(rehydrator: &mut EventRehydrator, block: &str) -> String {
    let mut streams = rehydrator.chat_arguments.take().expect("Chat Completions mode");
    let result = chat_event(rehydrator, &mut streams, block);
    rehydrator.chat_arguments = Some(streams);
    result
}

/// Flushes every pending argument suffix without touching text carries. Task
/// 2 uses this to place tool suffixes before a raw terminal tail.
pub(crate) fn flush_pending_arguments(rehydrator: &mut EventRehydrator) -> Vec<String> {
    let Some(mut streams) = rehydrator.chat_arguments.take() else { return Vec::new() };
    let (chunks, report) = flush_all_and_close(&mut streams);
    rehydrator.accumulate(report);
    rehydrator.chat_arguments = Some(streams);
    chunks
}

/// Entry point for `EventRehydrator::finish` in Chat mode.
pub(crate) fn finish_chat(rehydrator: &mut EventRehydrator) -> Vec<String> {
    let mut events = flush_pending_arguments(rehydrator);
    events.extend(rehydrator.flush_all_carries());
    events
}

struct Line<'a> {
    content: &'a str,
    ending: &'a str,
}

/// Splits a block on LF, CRLF, and lone CR while retaining every terminator.
fn split_lines(block: &str) -> Vec<Line<'_>> {
    let bytes = block.as_bytes();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        let end = match bytes[index] {
            b'\n' => index + 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => index + 2,
            b'\r' => index + 1,
            _ => {
                index += 1;
                continue;
            }
        };
        lines.push(Line { content: &block[start..index], ending: &block[index..end] });
        index = end;
        start = end;
    }
    if start < bytes.len() {
        lines.push(Line { content: &block[start..], ending: "" });
    }
    lines
}

fn splice_line(block: &str, lines: &[Line<'_>], target: usize, replacement: &str) -> String {
    let mut out = String::with_capacity(block.len() + replacement.len());
    for (index, line) in lines.iter().enumerate() {
        out.push_str(if index == target { replacement } else { line.content });
        out.push_str(line.ending);
    }
    out
}

fn chat_event(
    rehydrator: &mut EventRehydrator,
    streams: &mut ArgumentStreams,
    block: &str,
) -> String {
    let lines = split_lines(block);
    let mut data_index = None;
    let mut data_count = 0usize;
    let mut error_control = false;
    for (index, line) in lines.iter().enumerate() {
        let data = line.content.strip_prefix("data:").map(str::trim);
        // A bare `data` line is an SSE data field with an empty value.
        if let Some(data) = data.or_else(|| (line.content == "data").then_some("")) {
            data_count += 1;
            data_index.get_or_insert((index, data));
        } else if let Some(event) = line.content.strip_prefix("event:") {
            if event.trim() == "error" {
                error_control = true;
            }
        }
    }
    if error_control || data_count > 1 {
        return raw_event(rehydrator, streams, block);
    }
    let Some((data_line, payload)) = data_index else {
        return block.to_string();
    };
    if payload == "[DONE]" {
        let mut prelude = flush_arguments(rehydrator, streams);
        prelude.extend(rehydrator.flush_all_carries());
        return prepend(prelude, block);
    }
    let mut json: Value = match serde_json::from_str(payload) {
        Ok(json) => json,
        Err(_) => return raw_event(rehydrator, streams, block),
    };
    if (json.get("choices").is_some() || json.get("error").is_some())
        && has_ambiguous_chat_tool_members(payload)
    {
        return raw_event(rehydrator, streams, block);
    }
    let plan = match shape::plan_event(&mut json, streams) {
        EventPlan::Process(plan) => plan,
        EventPlan::Opaque => return raw_event(rehydrator, streams, block),
    };
    let (data_rendered, prelude) =
        chat_json_event(rehydrator, streams, lines[data_line].content, json, plan);
    let body = if data_rendered == lines[data_line].content {
        block.to_string()
    } else {
        splice_line(block, &lines, data_line, &data_rendered)
    };
    prepend(prelude, &body)
}

fn chat_json_event(
    rehydrator: &mut EventRehydrator,
    streams: &mut ArgumentStreams,
    line: &str,
    mut json: Value,
    mut plan: ProcessPlan,
) -> (String, Vec<String>) {
    let mut prelude = Vec::new();
    let mut report = RehydrateReport::default();
    let mut changed = false;
    let mut fenced_choices = Vec::new();
    if !plan.global_gap {
        // Choices fenced by this event will reclaim their tombstones when
        // flushed; do it before unrelated quarantines consult the pair cap.
        for choice in &plan.choices {
            if let (true, Some(index)) = (choice.finish || choice.quarantined, choice.index) {
                streams.reclaim_choice_tombstones(index);
                fenced_choices.push(index);
            }
        }
    }
    let mut suffixes = if plan.global_gap {
        streams.flush_all_records()
    } else {
        let mut records = Vec::new();
        for key in &plan.flush_pairs {
            records.extend(streams.flush_pair_records(*key, !fenced_choices.contains(&key.choice)));
        }
        for choice in &plan.flush_choices {
            records.extend(streams.flush_choice_records(*choice));
        }
        records
    };
    let mut detached = plan.detached;
    for entry in &detached {
        if !entry.process {
            if let Value::String(text) = &entry.value {
                report.unrestored = report
                    .unrestored
                    .saturating_add(count_unrestored_markers(text.as_bytes(), &rehydrator.map));
            }
        }
    }
    if !plan.global_gap {
        let candidate = EnvelopeCandidate::new(&json);
        // Finishing choices run first so a released pair frees capacity
        // before any new admission in the same event.
        for finishing in [true, false] {
            for choice in plan
                .choices
                .iter_mut()
                .filter(|choice| choice.finish == finishing && !choice.quarantined)
            {
                let tools = std::mem::take(&mut choice.tools);
                let mut processed = Vec::new();
                for tool in tools {
                    let Value::String(text) =
                        std::mem::replace(&mut detached[tool.writeback].value, Value::Null)
                    else {
                        unreachable!("planned tool holds a string")
                    };
                    let delta =
                        streams.push(tool.key, &text, &rehydrator.map, &candidate, finishing);
                    add_report(&mut report, delta.report);
                    let output = match delta.text {
                        Cow::Borrowed(_) => text,
                        Cow::Owned(output) => {
                            if output != text {
                                changed = true;
                            }
                            output
                        }
                    };
                    detached[tool.writeback].value = Value::String(output);
                    processed.push((tool.key, tool.writeback));
                }
                if finishing {
                    let Some(choice_index) = choice.index else { continue };
                    for (key, writeback) in processed {
                        let Some((suffix, unrestored)) = streams.take_final_suffix(key) else {
                            continue;
                        };
                        report.unrestored = report.unrestored.saturating_add(unrestored);
                        if let Value::String(current) = &detached[writeback].value {
                            let mut combined = String::with_capacity(current.len() + suffix.len());
                            combined.push_str(current);
                            combined.push_str(&suffix);
                            detached[writeback].value = Value::String(combined);
                            changed = true;
                        }
                    }
                    suffixes.extend(streams.flush_choice_records(choice_index));
                }
            }
        }
    }
    for (_, _, unrestored) in &suffixes {
        report.unrestored = report.unrestored.saturating_add(*unrestored);
    }
    prelude.extend(streams.render_suffixes(suffixes));
    if plan.global_gap {
        streams.envelope = None;
    }
    let mut terminal_keys = Vec::new();
    let mut saved_contents = Vec::new();
    if let Some(choices) = json.get_mut("choices").and_then(Value::as_array_mut) {
        for (position, choice) in choices.iter_mut().enumerate() {
            let key = super::super::CarryKey::ChatChoice(
                choice.get("index").and_then(Value::as_i64).unwrap_or(position as i64),
            );
            if choice.get("finish_reason").is_some_and(|reason| !reason.is_null()) {
                terminal_keys.push(key);
            }
            let content = choice
                .pointer_mut("/delta/content")
                .and_then(|value| value.as_str().map(str::to_string));
            if let Some(content) = content {
                if let Some(slot) = choice.pointer_mut("/delta/content") {
                    *slot = Value::String(String::new());
                }
                saved_contents.push((position, key, content));
            }
        }
    }
    let (mut restored, leaf) = crate::rehydrate::rehydrate_value(json, &rehydrator.map);
    add_report(&mut report, leaf);
    changed |= leaf.restored > 0;
    for (position, key, content) in saved_contents {
        let processed = rehydrator.process_delta_content(key, &content);
        changed |= processed != content;
        if let Some(target) = restored.pointer_mut(&format!("/choices/{position}/delta/content")) {
            *target = Value::String(processed);
        }
    }
    for entry in detached {
        let path =
            format!("/choices/{}/delta/tool_calls/{}/function/arguments", entry.choice, entry.tool);
        if let Some(slot) = restored.pointer_mut(&path) {
            *slot = entry.value;
        }
    }
    for entry in plan.detached_tool_calls {
        report.unrestored = report
            .unrestored
            .saturating_add(shape::count_bypassed_argument_markers(&entry.value, &rehydrator.map));
        if let Some(slot) =
            restored.pointer_mut(&format!("/choices/{}/delta/tool_calls", entry.choice))
        {
            *slot = entry.value;
        }
    }
    rehydrator.accumulate(report);
    prelude.extend(rehydrator.flush_carries(&terminal_keys));
    if !changed {
        return (line.to_string(), prelude);
    }
    match serde_json::to_string(&restored) {
        Ok(encoded) => (format!("data: {encoded}"), prelude),
        Err(_) => (line.to_string(), prelude),
    }
}

fn raw_event(
    rehydrator: &mut EventRehydrator,
    streams: &mut ArgumentStreams,
    block: &str,
) -> String {
    let prelude = flush_arguments(rehydrator, streams);
    rehydrator.note_unrestored_markers(block.as_bytes());
    prepend(prelude, block)
}

fn flush_arguments(rehydrator: &mut EventRehydrator, streams: &mut ArgumentStreams) -> Vec<String> {
    let (chunks, report) = flush_all_and_close(streams);
    rehydrator.accumulate(report);
    chunks
}

fn add_report(target: &mut RehydrateReport, source: RehydrateReport) {
    target.restored = target.restored.saturating_add(source.restored);
    target.unrestored = target.unrestored.saturating_add(source.unrestored);
}

fn prepend(prelude: Vec<String>, body: &str) -> String {
    if prelude.is_empty() {
        body.to_string()
    } else {
        format!("{}\n\n{}", prelude.join("\n\n"), body)
    }
}
