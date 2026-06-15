use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEditError {
    EmptyText,
    DeletedSegment,
    LineBreak,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegment {
    pub id: u32,
    pub start_sample: u64,
    pub end_sample: u64,
    pub sample_rate: u32,
    pub original_text: String,
    pub edited_text: String,
    pub retained: bool,
}

impl TranscriptSegment {
    pub fn new(
        id: u32,
        start_sample: u64,
        end_sample: u64,
        sample_rate: u32,
        text: String,
    ) -> Self {
        Self {
            id,
            start_sample,
            end_sample,
            sample_rate,
            original_text: text.clone(),
            edited_text: text,
            retained: true,
        }
    }

    pub fn start_seconds(&self) -> f64 {
        self.start_sample as f64 / self.sample_rate as f64
    }

    pub fn end_seconds(&self) -> f64 {
        self.end_sample as f64 / self.sample_rate as f64
    }

    pub fn edit_text(&mut self, text: &str) -> Result<(), TranscriptEditError> {
        if !self.retained {
            return Err(TranscriptEditError::DeletedSegment);
        }
        let text = text.trim();
        if text.is_empty() {
            return Err(TranscriptEditError::EmptyText);
        }
        if text.contains(['\n', '\r']) {
            return Err(TranscriptEditError::LineBreak);
        }
        self.edited_text = text.to_owned();
        Ok(())
    }

    pub fn delete(&mut self) {
        self.retained = false;
    }

    pub fn restore(&mut self) {
        self.retained = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{TranscriptEditError, TranscriptSegment};

    #[test]
    fn derives_seconds_from_sample_positions() {
        let segment = TranscriptSegment::new(1, 8_000, 24_000, 16_000, "测试".into());

        assert_eq!(segment.start_seconds(), 0.5);
        assert_eq!(segment.end_seconds(), 1.5);
        assert!(segment.retained);
        assert_eq!(segment.original_text, segment.edited_text);
    }

    #[test]
    fn edits_trimmed_non_empty_text() {
        let mut segment = TranscriptSegment::new(1, 0, 100, 16_000, "原文".into());

        segment.edit_text("  修正文本  ").expect("edit text");

        assert_eq!(segment.original_text, "原文");
        assert_eq!(segment.edited_text, "修正文本");
    }

    #[test]
    fn rejects_empty_text() {
        let mut segment = TranscriptSegment::new(1, 0, 100, 16_000, "原文".into());

        assert_eq!(
            segment.edit_text(" \n "),
            Err(TranscriptEditError::EmptyText)
        );
        assert_eq!(segment.edited_text, "原文");
    }

    #[test]
    fn rejects_line_breaks() {
        let mut segment = TranscriptSegment::new(1, 0, 100, 16_000, "原文".into());

        assert_eq!(
            segment.edit_text("第一行\n第二行"),
            Err(TranscriptEditError::LineBreak)
        );
        assert_eq!(
            segment.edit_text("第一行\r第二行"),
            Err(TranscriptEditError::LineBreak)
        );
        assert_eq!(segment.edited_text, "原文");
    }

    #[test]
    fn deleted_segment_must_be_restored_before_editing() {
        let mut segment = TranscriptSegment::new(1, 0, 100, 16_000, "原文".into());
        segment.delete();

        assert_eq!(
            segment.edit_text("修正"),
            Err(TranscriptEditError::DeletedSegment)
        );

        segment.restore();
        segment.edit_text("修正").expect("edit restored segment");
        assert_eq!(segment.edited_text, "修正");
    }
}
