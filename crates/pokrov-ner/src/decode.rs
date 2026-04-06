#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BioSpan {
    pub entity_label: String,
    pub token_start: usize,
    pub token_end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntitySpan {
    pub label: String,
    pub token_start: usize,
    pub token_end: usize,
    /// UTF-8 byte offset into the source text (matching regex crate convention).
    pub byte_start: usize,
    /// UTF-8 byte offset into the source text (matching regex crate convention).
    pub byte_end: usize,
    pub text: String,
}

pub fn decode_bio_tags(
    labels: &[String],
    char_offsets: &[(usize, usize)],
    text: &str,
) -> Vec<RawEntitySpan> {
    let mut spans = Vec::new();
    let mut current: Option<BioSpan> = None;

    for (i, label) in labels.iter().enumerate() {
        if let Some(stripped) = label.strip_prefix("B-") {
            if let Some(span) = current.take() {
                push_span(&span, char_offsets, text, &mut spans);
            }
            current =
                Some(BioSpan { entity_label: stripped.to_string(), token_start: i, token_end: i });
        } else if let Some(stripped) = label.strip_prefix("I-") {
            if let Some(ref mut span) = current {
                if span.entity_label == stripped {
                    span.token_end = i;
                } else {
                    push_span(span, char_offsets, text, &mut spans);
                    *span = BioSpan {
                        entity_label: stripped.to_string(),
                        token_start: i,
                        token_end: i,
                    };
                }
            } else {
                current = Some(BioSpan {
                    entity_label: stripped.to_string(),
                    token_start: i,
                    token_end: i,
                });
            }
        } else {
            if let Some(span) = current.take() {
                push_span(&span, char_offsets, text, &mut spans);
            }
        }
    }

    if let Some(span) = current.take() {
        push_span(&span, char_offsets, text, &mut spans);
    }

    spans
}

fn push_span(span: &BioSpan, offsets: &[(usize, usize)], text: &str, out: &mut Vec<RawEntitySpan>) {
    if span.token_start >= offsets.len() || span.token_end >= offsets.len() {
        return;
    }
    let byte_start = offsets[span.token_start].0;
    let byte_end = offsets[span.token_end].1;

    let extracted = if byte_start <= byte_end && byte_end <= text.len() {
        text[byte_start..byte_end].to_string()
    } else {
        String::new()
    };

    out.push(RawEntitySpan {
        label: span.entity_label.clone(),
        token_start: span.token_start,
        token_end: span.token_end,
        byte_start,
        byte_end,
        text: extracted,
    });
}

pub fn argmax_labels(logits: &ndarray::Array3<f32>) -> Vec<usize> {
    let (_, seq_len, num_labels) = logits.dim();
    let mut labels = Vec::with_capacity(seq_len);

    for t in 0..seq_len {
        let mut max_val = f32::NEG_INFINITY;
        let mut max_idx = 0usize;
        for l in 0..num_labels {
            let val = logits[[0, t, l]];
            if val > max_val {
                max_val = val;
                max_idx = l;
            }
        }
        labels.push(max_idx);
    }

    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_person_entity() {
        let labels = vec!["O".into(), "B-PER".into(), "I-PER".into(), "O".into()];
        let offsets = vec![(0, 1), (2, 6), (7, 12), (13, 18)];
        let text = "A John Smithson X";

        let spans = decode_bio_tags(&labels, &offsets, text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[0].byte_start, 2);
        assert_eq!(spans[0].byte_end, 12);
        assert_eq!(spans[0].text, "John Smith");
    }

    #[test]
    fn two_separate_entities() {
        let labels = vec!["O".into(), "B-PER".into(), "O".into(), "B-ORG".into(), "O".into()];
        let offsets = vec![(0, 1), (2, 6), (7, 8), (9, 15), (16, 20)];
        let text = "A John X Acme Corpxxxx";

        let spans = decode_bio_tags(&labels, &offsets, text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[0].text, "John");
        assert_eq!(spans[1].label, "ORG");
        assert_eq!(spans[1].text, "Acme C");
    }

    #[test]
    fn no_entities() {
        let labels = vec!["O".into(), "O".into(), "O".into()];
        let offsets = vec![(0, 5), (6, 10), (11, 15)];
        let text = "hello world test";

        let spans = decode_bio_tags(&labels, &offsets, text);
        assert!(spans.is_empty());
    }

    #[test]
    fn i_without_b_treated_as_new_entity() {
        let labels = vec!["O".into(), "I-PER".into(), "I-PER".into(), "O".into()];
        let offsets = vec![(0, 1), (2, 6), (7, 12), (13, 18)];
        let text = "A John Smithson X";

        let spans = decode_bio_tags(&labels, &offsets, text);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].byte_start, 2);
        assert_eq!(spans[0].byte_end, 12);
        assert_eq!(spans[0].text, "John Smith");
    }

    #[test]
    fn label_switch_starts_new_entity() {
        let labels = vec!["B-PER".into(), "I-ORG".into(), "O".into()];
        let offsets = vec![(0, 5), (6, 12), (13, 18)];
        let text = "Hello World! Something";

        let spans = decode_bio_tags(&labels, &offsets, text);
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].label, "PER");
        assert_eq!(spans[0].text, "Hello");
        assert_eq!(spans[1].label, "ORG");
        assert_eq!(spans[1].text, "World!");
    }

    #[test]
    fn argmax_picks_highest_logit() {
        let logits = ndarray::Array3::from_shape_vec(
            (1, 3, 3),
            vec![0.1, 0.5, 0.3, 0.9, 0.1, 0.0, 0.0, 0.0, 0.8],
        )
        .unwrap();

        let labels = argmax_labels(&logits);
        assert_eq!(labels, vec![1, 0, 2]);
    }
}
