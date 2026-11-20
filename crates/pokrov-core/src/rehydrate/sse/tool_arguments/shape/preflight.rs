//! Source-level duplicate-member detection along argument-association paths.

use std::cell::Cell;

use serde::de::{
    self, Deserialize, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor,
};

use super::super::MAX_EVENT_ENTRIES;

/// Whether a Chat Completions `data:` payload repeats any member along the
/// argument-association paths (`choices`, `index`, `delta`, `tool_calls`,
/// `function`, `arguments`, `finish_reason` in their respective objects).
/// A regular `Value` parse discards duplicates, so callers must decide
/// opacity from the source bytes. The payload is expected to be valid JSON;
/// parse failures return `true` so an untrusted shape is never acted on.
pub fn has_ambiguous_chat_tool_members(payload: &str) -> bool {
    let budget = ProbeBudget::default();
    let mut de = serde_json::Deserializer::from_str(payload);
    match (RootSeed { budget: &budget }).deserialize(&mut de) {
        Ok(ambiguous) => ambiguous || de.end().is_err(),
        Err(_) => true,
    }
}

// Probes descend only into members that can change argument association;
// unrelated branches are consumed with `IgnoredAny`. A shared budget bounds
// choices plus tool-call entries independently of array shape.

#[derive(Default)]
struct ProbeBudget {
    used: Cell<usize>,
}

impl ProbeBudget {
    fn charge(&self, extra: usize) -> Result<(), ()> {
        let total = self.used.get().saturating_add(extra);
        self.used.set(total);
        if total > MAX_EVENT_ENTRIES {
            Err(())
        } else {
            Ok(())
        }
    }
}

fn budget_error<E: de::Error>(_: ()) -> E {
    E::custom("chat entry budget exceeded")
}

macro_rules! probe_scalars {
    () => {
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_string<E: de::Error>(self, _: String) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_unit<E: de::Error>(self) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_none<E: de::Error>(self) -> Result<bool, E> {
            Ok(false)
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
            d.deserialize_any(self)
        }
    };
}

fn ignore_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<bool, A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(false)
}

fn ignore_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<bool, A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(false)
}

struct IgnoredSeed<'a> {
    budget: &'a ProbeBudget,
}

impl<'de, 'a> DeserializeSeed<'de> for IgnoredSeed<'a> {
    type Value = bool;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
        let _ = self.budget;
        IgnoredAny::deserialize(d).map(|_| false)
    }
}

// Member names are identified without allocating: serde_json calls
// `visit_str` for borrowed or decoded object keys, including escaped names.
const FIELD_OTHER: u8 = 0;
const FIELD_CHOICES: u8 = 1;
const FIELD_INDEX: u8 = 2;
const FIELD_FINISH_REASON: u8 = 3;
const FIELD_DELTA: u8 = 4;
const FIELD_TOOL_CALLS: u8 = 5;
const FIELD_FUNCTION: u8 = 6;
const FIELD_ARGUMENTS: u8 = 7;

#[derive(Clone, Copy)]
struct FieldKey(u8);

struct FieldKeyVisitor;

impl<'de> Visitor<'de> for FieldKeyVisitor {
    type Value = FieldKey;
    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a field name")
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<FieldKey, E> {
        Ok(FieldKey(match value {
            "choices" => FIELD_CHOICES,
            "index" => FIELD_INDEX,
            "finish_reason" => FIELD_FINISH_REASON,
            "delta" => FIELD_DELTA,
            "tool_calls" => FIELD_TOOL_CALLS,
            "function" => FIELD_FUNCTION,
            "arguments" => FIELD_ARGUMENTS,
            _ => FIELD_OTHER,
        }))
    }
}
impl<'de> Deserialize<'de> for FieldKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_identifier(FieldKeyVisitor)
    }
}
// Charges one entry before the wrapped seed inspects it. Used when the
// sequence has no size hint, so the budget bounds inspection rather than
// buffered forwarding.
struct ChargedSeed<'a, S> {
    budget: &'a ProbeBudget,
    seed: S,
}
impl<'de, 'a, S: DeserializeSeed<'de>> DeserializeSeed<'de> for ChargedSeed<'a, S> {
    type Value = S::Value;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        self.budget.charge(1).map_err(budget_error::<D::Error>)?;
        self.seed.deserialize(d)
    }
}
macro_rules! map_probe {
    ($visitor:ident, $seed:ident, $expect:literal, [$($key:pat => $mask:expr => $sub:ident),*]) => {
        struct $seed<'a> { budget: &'a ProbeBudget }
        impl<'de, 'a> DeserializeSeed<'de> for $seed<'a> {
            type Value = bool;
            fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
                d.deserialize_any($visitor { budget: self.budget })
            }
        }
        struct $visitor<'a> { budget: &'a ProbeBudget }
        impl<'de, 'a> Visitor<'de> for $visitor<'a> {
            type Value = bool;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str($expect)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<bool, A::Error> {
                let mut seen = 0u32;
                let mut ambiguous = false;
                while let Some(field) = map.next_key::<FieldKey>()? {
                    match field.0 {
                        $($key => {
                            ambiguous |= seen & $mask != 0;
                            seen |= $mask;
                            ambiguous |= map.next_value_seed($sub { budget: self.budget })?;
                        })*
                        _ => { map.next_value::<IgnoredAny>()?; }
                    }
                }
                Ok(ambiguous)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<bool, A::Error> {
                ignore_seq(seq)
            }
            probe_scalars!();
        }
    };
}
macro_rules! seq_probe {
    ($visitor:ident, $seed:ident, $expect:literal, $element:ident) => {
        struct $seed<'a> {
            budget: &'a ProbeBudget,
        }
        impl<'de, 'a> DeserializeSeed<'de> for $seed<'a> {
            type Value = bool;
            fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
                d.deserialize_any($visitor { budget: self.budget })
            }
        }
        struct $visitor<'a> {
            budget: &'a ProbeBudget,
        }
        impl<'de, 'a> Visitor<'de> for $visitor<'a> {
            type Value = bool;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str($expect)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<bool, A::Error> {
                let charge_each = match seq.size_hint() {
                    Some(len) => {
                        self.budget.charge(len).map_err(budget_error::<A::Error>)?;
                        false
                    }
                    None => true,
                };
                let mut ambiguous = false;
                while let Some(found) = if charge_each {
                    seq.next_element_seed(ChargedSeed {
                        budget: self.budget,
                        seed: $element { budget: self.budget },
                    })?
                } else {
                    seq.next_element_seed($element { budget: self.budget })?
                } {
                    ambiguous |= found;
                }
                Ok(ambiguous)
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<bool, A::Error> {
                ignore_map(map)
            }
            probe_scalars!();
        }
    };
}
map_probe!(RootVisitor, RootSeed, "a JSON object", [FIELD_CHOICES => 1 => ChoicesSeed]);
seq_probe!(ChoicesVisitor, ChoicesSeed, "a choices array", ChoiceSeed);
map_probe!(ChoiceVisitor, ChoiceSeed, "a choice object", [FIELD_INDEX => 1 => IgnoredSeed, FIELD_FINISH_REASON => 2 => IgnoredSeed, FIELD_DELTA => 4 => DeltaSeed]);
map_probe!(DeltaVisitor, DeltaSeed, "a delta object", [FIELD_TOOL_CALLS => 1 => ToolCallsSeed]);
seq_probe!(ToolCallsVisitor, ToolCallsSeed, "a tool call array", ToolCallSeed);
map_probe!(ToolCallVisitor, ToolCallSeed, "a tool call object", [FIELD_INDEX => 1 => IgnoredSeed, FIELD_FUNCTION => 2 => FunctionSeed]);
map_probe!(FunctionVisitor, FunctionSeed, "a function object", [FIELD_ARGUMENTS => 1 => IgnoredSeed]);
