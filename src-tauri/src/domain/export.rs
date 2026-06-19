use std::error::Error;
use std::fmt;
use std::ops::Range;

use serde::Serialize;

use super::transcript::TranscriptSegment;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleRange {
    pub start_sample: u64,
    pub end_sample: u64,
}

impl SampleRange {
    pub fn duration_samples(self) -> u64 {
        self.end_sample - self.start_sample
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCue {
    pub index: u32,
    pub start_sample: u64,
    pub end_sample: u64,
    pub sample_rate: u32,
    pub text: String,
}

impl ExportCue {
    pub fn start_seconds(&self) -> f64 {
        self.start_sample as f64 / self.sample_rate as f64
    }

    pub fn end_seconds(&self) -> f64 {
        self.end_sample as f64 / self.sample_rate as f64
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportTimeline {
    pub ranges: Vec<SampleRange>,
    pub cues: Vec<ExportCue>,
    pub sample_rate: u32,
}

impl ExportTimeline {
    pub fn duration_samples(&self) -> u64 {
        self.ranges
            .iter()
            .copied()
            .map(SampleRange::duration_samples)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportTimelineError {
    NoRetainedSegments,
    InvalidSegment {
        id: u32,
        start_sample: u64,
        end_sample: u64,
    },
    SampleRateMismatch {
        expected: u32,
        actual: u32,
        id: u32,
    },
}

impl fmt::Display for ExportTimelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRetainedSegments => write!(formatter, "至少需要保留一个字幕片段"),
            Self::InvalidSegment {
                id,
                start_sample,
                end_sample,
            } => write!(
                formatter,
                "字幕片段 {id} 的时间范围无效：{start_sample}..{end_sample}"
            ),
            Self::SampleRateMismatch {
                expected,
                actual,
                id,
            } => write!(
                formatter,
                "字幕片段 {id} 的采样率为 {actual} Hz，预期为 {expected} Hz"
            ),
        }
    }
}

impl Error for ExportTimelineError {}

pub fn build_export_timeline(
    segments: &[TranscriptSegment],
) -> Result<ExportTimeline, ExportTimelineError> {
    let mut retained: Vec<&TranscriptSegment> =
        segments.iter().filter(|segment| segment.retained).collect();
    if retained.is_empty() {
        return Err(ExportTimelineError::NoRetainedSegments);
    }
    retained.sort_by_key(|segment| (segment.start_sample, segment.end_sample, segment.id));

    let sample_rate = retained[0].sample_rate;
    let mut ranges = Vec::new();
    for segment in &retained {
        validate_segment(segment, sample_rate)?;
        merge_range(
            &mut ranges,
            SampleRange {
                start_sample: segment.start_sample,
                end_sample: segment.end_sample,
            },
        );
    }

    let mut cues = Vec::new();
    for segment in retained {
        let (range, preceding_duration) = containing_range(&ranges, segment);
        push_split_cues(
            &mut cues,
            preceding_duration + segment.start_sample - range.start_sample,
            preceding_duration + segment.end_sample - range.start_sample,
            sample_rate,
            &segment.edited_text,
        );
    }

    Ok(ExportTimeline {
        ranges,
        cues,
        sample_rate,
    })
}

pub fn build_original_subtitle_cues(
    segments: &[TranscriptSegment],
) -> Result<Vec<ExportCue>, ExportTimelineError> {
    let mut retained: Vec<&TranscriptSegment> =
        segments.iter().filter(|segment| segment.retained).collect();
    if retained.is_empty() {
        return Err(ExportTimelineError::NoRetainedSegments);
    }
    retained.sort_by_key(|segment| (segment.start_sample, segment.end_sample, segment.id));

    let sample_rate = retained[0].sample_rate;
    let mut cues = Vec::new();
    for segment in retained {
        validate_segment(segment, sample_rate)?;
        push_split_cues(
            &mut cues,
            segment.start_sample,
            segment.end_sample,
            sample_rate,
            &segment.edited_text,
        );
    }
    Ok(cues)
}

fn validate_segment(
    segment: &TranscriptSegment,
    expected_sample_rate: u32,
) -> Result<(), ExportTimelineError> {
    if segment.end_sample <= segment.start_sample {
        return Err(ExportTimelineError::InvalidSegment {
            id: segment.id,
            start_sample: segment.start_sample,
            end_sample: segment.end_sample,
        });
    }
    if segment.sample_rate != expected_sample_rate {
        return Err(ExportTimelineError::SampleRateMismatch {
            expected: expected_sample_rate,
            actual: segment.sample_rate,
            id: segment.id,
        });
    }
    Ok(())
}

fn merge_range(ranges: &mut Vec<SampleRange>, next: SampleRange) {
    if let Some(current) = ranges.last_mut() {
        if next.start_sample <= current.end_sample {
            current.end_sample = current.end_sample.max(next.end_sample);
            return;
        }
    }
    ranges.push(next);
}

fn containing_range<'a>(
    ranges: &'a [SampleRange],
    segment: &TranscriptSegment,
) -> (&'a SampleRange, u64) {
    let mut preceding_duration = 0;
    for range in ranges {
        if segment.start_sample >= range.start_sample && segment.end_sample <= range.end_sample {
            return (range, preceding_duration);
        }
        preceding_duration += range.duration_samples();
    }
    unreachable!("validated segment must be contained in a merged export range")
}

fn push_split_cues(
    cues: &mut Vec<ExportCue>,
    start_sample: u64,
    end_sample: u64,
    sample_rate: u32,
    text: &str,
) {
    for (start_sample, end_sample, text) in split_cue(start_sample, end_sample, text) {
        cues.push(ExportCue {
            index: cues.len() as u32 + 1,
            start_sample,
            end_sample,
            sample_rate,
            text,
        });
    }
}

fn split_cue(start_sample: u64, end_sample: u64, text: &str) -> Vec<(u64, u64, String)> {
    let ranges = split_text_ranges(text);
    if ranges.len() <= 1 || end_sample - start_sample < ranges.len() as u64 {
        return vec![(start_sample, end_sample, text.trim().to_owned())];
    }

    let weights = ranges
        .iter()
        .map(|range| content_character_count(&text[range.clone()]))
        .collect::<Vec<_>>();
    let total_weight = weights.iter().sum::<usize>();
    if total_weight == 0 {
        return vec![(start_sample, end_sample, text.trim().to_owned())];
    }

    let duration = end_sample - start_sample;
    let mut output = Vec::with_capacity(ranges.len());
    let mut current_start = start_sample;
    let mut accumulated_weight = 0usize;
    for (index, range) in ranges.into_iter().enumerate() {
        let current_end = if index + 1 == weights.len() {
            end_sample
        } else {
            accumulated_weight += weights[index];
            let proportional = start_sample
                + ((duration as u128 * accumulated_weight as u128) / total_weight as u128) as u64;
            let remaining_cues = weights.len() - index - 1;
            proportional.clamp(current_start + 1, end_sample - remaining_cues as u64)
        };
        output.push((current_start, current_end, text[range].to_owned()));
        current_start = current_end;
    }
    output
}

fn split_text_ranges(text: &str) -> Vec<Range<usize>> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut start = 0;
    let characters = text.char_indices().collect::<Vec<_>>();
    for (position, (index, character)) in characters.iter().copied().enumerate() {
        if is_export_split_punctuation(&characters, position, character) {
            let end = index + character.len_utf8();
            push_trimmed_range(text, start..end, &mut ranges);
            start = end;
        }
    }
    push_trimmed_range(text, start..text.len(), &mut ranges);
    ranges
}

fn is_export_split_punctuation(
    characters: &[(usize, char)],
    position: usize,
    character: char,
) -> bool {
    if is_numeric_separator(characters, position, character) {
        return false;
    }
    matches!(character, '，' | ',' | '。' | '.' | '？' | '?' | '！' | '!')
}

fn is_numeric_separator(characters: &[(usize, char)], position: usize, character: char) -> bool {
    matches!(character, '.' | ',')
        && position > 0
        && position + 1 < characters.len()
        && characters[position - 1].1.is_ascii_digit()
        && characters[position + 1].1.is_ascii_digit()
}

fn content_character_count(text: &str) -> usize {
    text.chars()
        .filter(|character| character.is_alphanumeric())
        .count()
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

#[cfg(test)]
mod tests {
    use super::{
        build_export_timeline, build_original_subtitle_cues, ExportTimelineError, SampleRange,
    };
    use crate::domain::transcript::TranscriptSegment;

    #[test]
    fn merges_overlapping_and_adjacent_ranges() {
        let segments = vec![
            segment(1, 100, 200, "一"),
            segment(2, 180, 250, "二"),
            segment(3, 250, 300, "三"),
            segment(4, 400, 500, "四"),
        ];

        let timeline = build_export_timeline(&segments).expect("timeline");

        assert_eq!(
            timeline.ranges,
            vec![
                SampleRange {
                    start_sample: 100,
                    end_sample: 300,
                },
                SampleRange {
                    start_sample: 400,
                    end_sample: 500,
                }
            ]
        );
        assert_eq!(timeline.duration_samples(), 300);
    }

    #[test]
    fn recalculates_cues_after_removed_gaps() {
        let segments = vec![
            segment(1, 100, 200, "第一句"),
            segment(2, 180, 250, "第二句"),
            segment(3, 400, 500, "第三句"),
        ];

        let timeline = build_export_timeline(&segments).expect("timeline");

        assert_eq!(
            timeline
                .cues
                .iter()
                .map(|cue| (cue.index, cue.start_sample, cue.end_sample))
                .collect::<Vec<_>>(),
            vec![(1, 0, 100), (2, 80, 150), (3, 150, 250)]
        );
    }

    #[test]
    fn splits_export_cues_at_punctuation_without_a_minimum_length() {
        let timeline =
            build_export_timeline(&[segment(1, 0, 900, "好，行。结束")]).expect("timeline");

        assert_eq!(
            timeline
                .cues
                .iter()
                .map(|cue| (
                    cue.index,
                    cue.start_sample,
                    cue.end_sample,
                    cue.text.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                (1, 0, 225, "好，"),
                (2, 225, 450, "行。"),
                (3, 450, 900, "结束")
            ]
        );
        assert_eq!(
            timeline.ranges,
            vec![SampleRange {
                start_sample: 0,
                end_sample: 900,
            }]
        );
    }

    #[test]
    fn keeps_numeric_separators_when_splitting_export_cues() {
        let timeline = build_export_timeline(&[segment(1, 0, 1_000, "版本 2.5，数量 1,000！结束")])
            .expect("timeline");

        assert_eq!(
            timeline
                .cues
                .iter()
                .map(|cue| cue.text.as_str())
                .collect::<Vec<_>>(),
            vec!["版本 2.5，", "数量 1,000！", "结束"]
        );
    }

    #[test]
    fn splits_original_subtitle_cues_with_original_timestamps() {
        let cues = build_original_subtitle_cues(&[segment(1, 100, 1_000, "好，行。结束")])
            .expect("original cues");

        assert_eq!(
            cues.iter()
                .map(|cue| (
                    cue.index,
                    cue.start_sample,
                    cue.end_sample,
                    cue.text.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                (1, 100, 325, "好，"),
                (2, 325, 550, "行。"),
                (3, 550, 1_000, "结束")
            ]
        );
    }

    #[test]
    fn ignores_deleted_segments() {
        let mut deleted = segment(1, 0, 100, "删除");
        deleted.delete();
        let retained = segment(2, 200, 300, "保留");

        let timeline = build_export_timeline(&[deleted, retained]).expect("timeline");

        assert_eq!(timeline.ranges.len(), 1);
        assert_eq!(timeline.cues[0].text, "保留");
        assert_eq!(timeline.cues[0].index, 1);
    }

    #[test]
    fn keeps_adjacent_retained_segments_in_one_export_range() {
        let segments = vec![
            segment(1, 100, 200, "第一段"),
            segment(2, 200, 300, "第二段"),
            segment(3, 300, 400, "第三段"),
        ];

        let timeline = build_export_timeline(&segments).expect("timeline");

        assert_eq!(
            timeline.ranges,
            vec![SampleRange {
                start_sample: 100,
                end_sample: 400,
            }]
        );
        assert_eq!(
            timeline
                .cues
                .iter()
                .map(|cue| (cue.start_sample, cue.end_sample))
                .collect::<Vec<_>>(),
            vec![(0, 100), (100, 200), (200, 300)]
        );
    }

    #[test]
    fn removes_only_the_deleted_segment_range() {
        let first = segment(1, 100, 200, "第一段");
        let mut deleted = segment(2, 200, 300, "删除");
        deleted.delete();
        let third = segment(3, 300, 400, "第三段");

        let timeline = build_export_timeline(&[first, deleted, third]).expect("timeline");

        assert_eq!(
            timeline.ranges,
            vec![
                SampleRange {
                    start_sample: 100,
                    end_sample: 200,
                },
                SampleRange {
                    start_sample: 300,
                    end_sample: 400,
                }
            ]
        );
        assert_eq!(
            timeline
                .cues
                .iter()
                .map(|cue| (cue.start_sample, cue.end_sample))
                .collect::<Vec<_>>(),
            vec![(0, 100), (100, 200)]
        );
    }

    #[test]
    fn rejects_an_export_with_no_retained_segments() {
        let mut segment = segment(1, 0, 100, "删除");
        segment.delete();

        assert_eq!(
            build_export_timeline(&[segment]),
            Err(ExportTimelineError::NoRetainedSegments)
        );
    }

    #[test]
    fn rejects_mixed_sample_rates() {
        let first = segment(1, 0, 100, "一");
        let mut second = segment(2, 200, 300, "二");
        second.sample_rate = 48_000;

        assert!(matches!(
            build_export_timeline(&[first, second]),
            Err(ExportTimelineError::SampleRateMismatch { id: 2, .. })
        ));
    }

    #[test]
    fn keeps_original_gaps_for_subtitle_only_exports() {
        let first = segment(1, 100, 200, "第一句");
        let mut deleted = segment(2, 200, 400, "删除");
        deleted.delete();
        let third = segment(3, 400, 500, "第三句");

        let cues = build_original_subtitle_cues(&[third, deleted, first]).expect("original cues");

        assert_eq!(
            cues.iter()
                .map(|cue| (cue.index, cue.start_sample, cue.end_sample))
                .collect::<Vec<_>>(),
            vec![(1, 100, 200), (2, 400, 500)]
        );
    }

    fn segment(id: u32, start: u64, end: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment::new(id, start, end, 1_000, text.into())
    }
}
