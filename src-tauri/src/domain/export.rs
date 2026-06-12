use std::error::Error;
use std::fmt;

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

    let mut cues = Vec::with_capacity(retained.len());
    for (index, segment) in retained.into_iter().enumerate() {
        let (range, preceding_duration) = containing_range(&ranges, segment);
        cues.push(ExportCue {
            index: index as u32 + 1,
            start_sample: preceding_duration + segment.start_sample - range.start_sample,
            end_sample: preceding_duration + segment.end_sample - range.start_sample,
            sample_rate,
            text: segment.edited_text.clone(),
        });
    }

    Ok(ExportTimeline {
        ranges,
        cues,
        sample_rate,
    })
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

#[cfg(test)]
mod tests {
    use super::{build_export_timeline, ExportTimelineError, SampleRange};
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

    fn segment(id: u32, start: u64, end: u64, text: &str) -> TranscriptSegment {
        TranscriptSegment::new(id, start, end, 1_000, text.into())
    }
}
