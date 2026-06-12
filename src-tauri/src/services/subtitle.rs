use crate::domain::export::ExportCue;

pub fn render_srt(cues: &[ExportCue]) -> String {
    let mut output = String::new();
    for cue in cues {
        output.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            cue.index,
            format_timestamp(cue.start_sample, cue.sample_rate),
            format_timestamp(cue.end_sample, cue.sample_rate),
            cue.text.trim()
        ));
    }
    output
}

fn format_timestamp(sample: u64, sample_rate: u32) -> String {
    let total_milliseconds = sample.saturating_mul(1_000) / sample_rate as u64;
    let hours = total_milliseconds / 3_600_000;
    let minutes = total_milliseconds % 3_600_000 / 60_000;
    let seconds = total_milliseconds % 60_000 / 1_000;
    let milliseconds = total_milliseconds % 1_000;
    format!("{hours:02}:{minutes:02}:{seconds:02},{milliseconds:03}")
}

#[cfg(test)]
mod tests {
    use super::render_srt;
    use crate::domain::export::ExportCue;

    #[test]
    fn renders_recalculated_export_cues_as_srt() {
        let cues = vec![
            ExportCue {
                index: 1,
                start_sample: 500,
                end_sample: 1_750,
                sample_rate: 1_000,
                text: "第一句".into(),
            },
            ExportCue {
                index: 2,
                start_sample: 2_000,
                end_sample: 3_000,
                sample_rate: 1_000,
                text: "second line".into(),
            },
        ];

        assert_eq!(
            render_srt(&cues),
            "1\n00:00:00,500 --> 00:00:01,750\n第一句\n\n\
             2\n00:00:02,000 --> 00:00:03,000\nsecond line\n\n"
        );
    }
}
