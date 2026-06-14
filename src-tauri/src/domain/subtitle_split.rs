use std::ops::Range;

const MIN_CONTENT_CHARACTERS: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleSlice {
    pub start_sample: u64,
    pub end_sample: u64,
    pub text: String,
}

pub fn split_recognition(
    vad_start_sample: u64,
    vad_end_sample: u64,
    sample_rate: u32,
    text: &str,
    tokens: &[String],
    timestamps: Option<&[f32]>,
) -> Vec<SubtitleSlice> {
    let text = text.trim();
    let fallback = || {
        vec![SubtitleSlice {
            start_sample: vad_start_sample,
            end_sample: vad_end_sample,
            text: text.to_owned(),
        }]
    };
    let ranges = split_ranges(text);
    if ranges.len() <= 1 {
        return fallback();
    }

    let Some(timestamps) = timestamps.filter(|values| values.len() == tokens.len()) else {
        return fallback();
    };
    let Some(token_spans) = align_tokens(text, tokens) else {
        return fallback();
    };

    let mut boundaries = Vec::with_capacity(ranges.len() - 1);
    for range in ranges.iter().skip(1) {
        let Some(token_index) = token_index_at(&token_spans, range.start) else {
            return fallback();
        };
        let timestamp = timestamps[token_index];
        if !timestamp.is_finite() || timestamp < 0.0 {
            return fallback();
        }

        let relative_sample = (timestamp as f64 * sample_rate as f64).round() as u64;
        let boundary = vad_start_sample
            .saturating_add(relative_sample)
            .min(vad_end_sample);
        if boundary <= boundaries.last().copied().unwrap_or(vad_start_sample)
            || boundary >= vad_end_sample
        {
            return fallback();
        }
        boundaries.push(boundary);
    }

    ranges
        .into_iter()
        .enumerate()
        .map(|(index, range)| SubtitleSlice {
            start_sample: if index == 0 {
                vad_start_sample
            } else {
                boundaries[index - 1]
            },
            end_sample: boundaries.get(index).copied().unwrap_or(vad_end_sample),
            text: text[range].to_owned(),
        })
        .collect()
}

fn split_ranges(text: &str) -> Vec<Range<usize>> {
    if text.is_empty() {
        return vec![0..0];
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    let mut content_characters = 0;

    for (index, character) in text.char_indices() {
        if character.is_alphanumeric() {
            content_characters += 1;
        }
        if is_split_punctuation(character) && content_characters >= MIN_CONTENT_CHARACTERS {
            let end = index + character.len_utf8();
            push_trimmed_range(text, start..end, &mut ranges);
            start = end;
            content_characters = 0;
        }
    }

    push_trimmed_range(text, start..text.len(), &mut ranges);
    ranges
}

fn is_split_punctuation(character: char) -> bool {
    matches!(
        character,
        '，' | ',' | '；' | ';' | '？' | '?' | '！' | '!' | '。' | '.'
    )
}

fn push_trimmed_range(text: &str, range: Range<usize>, ranges: &mut Vec<Range<usize>>) {
    let value = &text[range.clone()];
    let leading = value.len() - value.trim_start().len();
    let trailing = value.len() - value.trim_end().len();
    let trimmed = range.start + leading..range.end - trailing;
    if !trimmed.is_empty() {
        ranges.push(trimmed);
    }
}

#[derive(Debug, Clone, Copy)]
struct TokenSpan {
    index: usize,
    start: usize,
    end: usize,
}

fn align_tokens(text: &str, tokens: &[String]) -> Option<Vec<TokenSpan>> {
    [
        TokenRendering::Original,
        TokenRendering::SentencePieceAsSpace,
        TokenRendering::SentencePieceRemoved,
    ]
    .into_iter()
    .find_map(|rendering| render_and_align(text, tokens, rendering))
}

#[derive(Debug, Clone, Copy)]
enum TokenRendering {
    Original,
    SentencePieceAsSpace,
    SentencePieceRemoved,
}

fn render_and_align(
    text: &str,
    tokens: &[String],
    rendering: TokenRendering,
) -> Option<Vec<TokenSpan>> {
    let mut rendered = String::new();
    let mut spans = Vec::with_capacity(tokens.len());

    for (index, token) in tokens.iter().enumerate() {
        let start = rendered.len();
        match rendering {
            TokenRendering::Original => rendered.push_str(token),
            TokenRendering::SentencePieceAsSpace => {
                rendered.push_str(&token.replace('▁', " "));
            }
            TokenRendering::SentencePieceRemoved => {
                rendered.extend(token.chars().filter(|character| *character != '▁'));
            }
        }
        spans.push(TokenSpan {
            index,
            start,
            end: rendered.len(),
        });
    }

    let trimmed = rendered.trim();
    if trimmed != text {
        return None;
    }
    let leading = rendered.len() - rendered.trim_start().len();
    let text_end = leading + text.len();

    Some(
        spans
            .into_iter()
            .filter_map(|span| {
                let start = span.start.max(leading);
                let end = span.end.min(text_end);
                (start < end).then_some(TokenSpan {
                    index: span.index,
                    start: start - leading,
                    end: end - leading,
                })
            })
            .collect(),
    )
}

fn token_index_at(spans: &[TokenSpan], text_offset: usize) -> Option<usize> {
    spans
        .iter()
        .find(|span| span.start <= text_offset && text_offset < span.end)
        .map(|span| span.index)
}

#[cfg(test)]
mod tests {
    use super::{split_ranges, split_recognition, SubtitleSlice};

    const RATE: u32 = 16_000;

    #[test]
    fn splits_at_each_supported_chinese_and_english_punctuation() {
        for punctuation in ['，', ',', '；', ';', '？', '?', '！', '!', '。', '.'] {
            let text = format!("一二三四五六七八九十甲乙丙丁戊{punctuation}后续");
            assert_eq!(
                split_text(&text),
                vec![
                    format!("一二三四五六七八九十甲乙丙丁戊{punctuation}"),
                    "后续".to_owned()
                ],
                "punctuation: {punctuation}"
            );
        }
    }

    #[test]
    fn does_not_split_before_fifteen_content_characters() {
        let text = "一二三四五六七，八九十甲乙丙丁。";

        assert_eq!(split_text(text), vec![text]);
    }

    #[test]
    fn ignores_whitespace_and_all_punctuation_when_counting() {
        let text = "一二三四五，\n六七八：九十（甲）乙丙丁戊。剩余";

        assert_eq!(
            split_text(text),
            vec!["一二三四五，\n六七八：九十（甲）乙丙丁戊。", "剩余"]
        );
    }

    #[test]
    fn resets_the_character_count_after_each_split() {
        let text = concat!(
            "一二三四五六七八九十甲乙丙丁戊，",
            "一二三四五六七八九十甲乙丙丁戊；",
            "结尾"
        );

        assert_eq!(
            split_text(text),
            vec![
                "一二三四五六七八九十甲乙丙丁戊，",
                "一二三四五六七八九十甲乙丙丁戊；",
                "结尾"
            ]
        );
    }

    #[test]
    fn retains_a_short_final_chunk() {
        let text = "一二三四五六七八九十甲乙丙丁戊。短句";

        assert_eq!(
            split_text(text),
            vec!["一二三四五六七八九十甲乙丙丁戊。", "短句"]
        );
    }

    #[test]
    fn builds_continuous_sample_ranges_from_token_timestamps() {
        let first = "一二三四五六七八九十甲乙丙丁戊，";
        let second = "后续内容";
        let text = format!("{first}{second}");
        let tokens = text
            .chars()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        let timestamps = (0..tokens.len())
            .map(|index| index as f32 * 0.1)
            .collect::<Vec<_>>();

        let result = split_recognition(32_000, 96_000, RATE, &text, &tokens, Some(&timestamps));
        let boundary = 32_000 + first.chars().count() as u64 * 1_600;

        assert_eq!(
            result,
            vec![
                SubtitleSlice {
                    start_sample: 32_000,
                    end_sample: boundary,
                    text: first.into(),
                },
                SubtitleSlice {
                    start_sample: boundary,
                    end_sample: 96_000,
                    text: second.into(),
                }
            ]
        );
    }

    #[test]
    fn supports_sentencepiece_markers_when_aligning_english_tokens() {
        let first = "one two three four five,";
        let second = "next";
        let tokens = vec!["▁one", "▁two", "▁three", "▁four", "▁five", ",", "▁next"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let timestamps = vec![0.0, 0.2, 0.4, 0.6, 0.8, 1.0, 1.2];

        let result = split_recognition(
            0,
            32_000,
            RATE,
            &format!("{first} {second}"),
            &tokens,
            Some(&timestamps),
        );

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].text, first);
        assert_eq!(result[0].end_sample, 19_200);
        assert_eq!(result[1].start_sample, 19_200);
        assert_eq!(result[1].text, second);
    }

    #[test]
    fn falls_back_to_the_vad_segment_when_alignment_is_unavailable() {
        let text = "一二三四五六七八九十甲乙丙丁戊。后续";
        let result = split_recognition(100, 500, RATE, text, &["无法对齐".into()], Some(&[0.0]));

        assert_eq!(
            result,
            vec![SubtitleSlice {
                start_sample: 100,
                end_sample: 500,
                text: text.into(),
            }]
        );
    }

    #[test]
    fn falls_back_when_timestamps_are_missing_or_invalid() {
        let text = "一二三四五六七八九十甲乙丙丁戊。后续";
        let tokens = text
            .chars()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();

        for timestamps in [None, Some(&[0.0][..])] {
            let result = split_recognition(100, 500, RATE, text, &tokens, timestamps);
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].start_sample, 100);
            assert_eq!(result[0].end_sample, 500);
        }
    }

    fn split_text(text: &str) -> Vec<&str> {
        split_ranges(text)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }
}
