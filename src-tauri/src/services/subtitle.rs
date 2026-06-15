use crate::domain::export::ExportCue;

pub fn render_srt(cues: &[ExportCue]) -> String {
    let mut output = String::new();
    for cue in cues {
        let text = format_subtitle_text(&cue.text);
        output.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            cue.index,
            format_timestamp(cue.start_sample, cue.sample_rate),
            format_timestamp(cue.end_sample, cue.sample_rate),
            text
        ));
    }
    output
}

fn format_subtitle_text(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut characters = normalized.chars().collect::<Vec<_>>();
    let mut replaceable = characters
        .iter()
        .enumerate()
        .map(|(index, character)| {
            is_target_punctuation(*character)
                && !is_numeric_separator(&characters, index, *character)
        })
        .collect::<Vec<_>>();

    loop {
        while characters
            .last()
            .is_some_and(|character| character.is_whitespace())
        {
            characters.pop();
            replaceable.pop();
        }
        if replaceable.last() == Some(&true) {
            characters.pop();
            replaceable.pop();
        } else {
            break;
        }
    }

    let mut formatted = String::new();
    let mut index = 0;
    while index < characters.len() {
        if replaceable[index] {
            while formatted.ends_with(' ') {
                formatted.pop();
            }
            index += 1;
            while index < characters.len()
                && (replaceable[index] || characters[index].is_whitespace())
            {
                index += 1;
            }
            if !formatted.is_empty() && index < characters.len() {
                formatted.push_str("  ");
            }
            continue;
        }

        let character = characters[index];
        if character.is_whitespace() {
            if !formatted.is_empty() && !formatted.ends_with(' ') {
                formatted.push(' ');
            }
        } else {
            formatted.push(character);
        }
        index += 1;
    }

    wrap_long_subtitle(formatted.trim())
}

fn is_target_punctuation(character: char) -> bool {
    matches!(character, '，' | ',' | '。' | '.' | '？' | '?' | '！' | '!')
}

fn is_numeric_separator(characters: &[char], index: usize, character: char) -> bool {
    matches!(character, '.' | ',')
        && index > 0
        && index + 1 < characters.len()
        && characters[index - 1].is_ascii_digit()
        && characters[index + 1].is_ascii_digit()
}

fn wrap_long_subtitle(text: &str) -> String {
    let content_characters = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    if content_characters <= 26 {
        return text.to_owned();
    }

    let first_line_characters = content_characters.div_ceil(2);
    let mut seen = 0;
    let mut split_at = text.len();
    for (index, character) in text.char_indices() {
        if character.is_alphanumeric() {
            seen += 1;
            if seen == first_line_characters {
                split_at = index + character.len_utf8();
                break;
            }
        }
    }
    let mut extended_split = split_at;
    for character in text[split_at..].chars() {
        if character.is_alphanumeric() {
            break;
        }
        extended_split += character.len_utf8();
    }
    split_at = extended_split;

    let first = text[..split_at].trim_end();
    let second = text[split_at..].trim_start();
    format!("{first}\n{second}")
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
    use super::{format_subtitle_text, render_srt};
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

    #[test]
    fn replaces_only_confirmed_punctuation_and_removes_trailing_groups() {
        assert_eq!(format_subtitle_text("你好 ，！？ 世界？！"), "你好  世界");
        assert_eq!(
            format_subtitle_text("说明：测试；完成（保留）"),
            "说明：测试；完成（保留）"
        );
    }

    #[test]
    fn handles_each_confirmed_chinese_and_english_punctuation_mark() {
        for punctuation in ['，', ',', '。', '.', '？', '?', '！', '!'] {
            assert_eq!(
                format_subtitle_text(&format!("第一部分{punctuation}第二部分")),
                "第一部分  第二部分",
                "punctuation: {punctuation}"
            );
        }
    }

    #[test]
    fn preserves_decimal_points_and_numeric_commas() {
        assert_eq!(
            format_subtitle_text("版本是 2.5，数量是 1,000！"),
            "版本是 2.5  数量是 1,000"
        );
        assert_eq!(format_subtitle_text("版本 2.5.1，继续"), "版本 2.5.1  继续");
    }

    #[test]
    fn normalizes_existing_whitespace_before_export() {
        assert_eq!(
            format_subtitle_text("第一部分 \t 第二部分，  第三部分。"),
            "第一部分 第二部分  第三部分"
        );
    }

    #[test]
    fn wraps_only_when_content_exceeds_twenty_six_characters() {
        let twenty_six = "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳";
        let twenty_seven = format!("{twenty_six}午");

        assert_eq!(format_subtitle_text(twenty_six), twenty_six);
        assert_eq!(
            format_subtitle_text(&twenty_seven),
            "一二三四五六七八九十甲乙丙丁\n戊己庚辛壬癸子丑寅卯辰巳午"
        );
    }

    #[test]
    fn counts_only_letters_and_numbers_when_finding_the_middle() {
        let text = "一二三四五六七，八九十甲乙丙丁；戊己庚辛壬癸子丑寅卯辰巳午";
        let formatted = format_subtitle_text(text);
        let lines = formatted.lines().collect::<Vec<_>>();

        assert_eq!(lines.len(), 2);
        assert_eq!(
            lines[0]
                .chars()
                .filter(|character| character.is_alphanumeric())
                .count(),
            14
        );
        assert_eq!(
            lines[1]
                .chars()
                .filter(|character| character.is_alphanumeric())
                .count(),
            13
        );
        assert!(lines[0].ends_with('；'));
    }

    #[test]
    fn rendering_does_not_modify_the_export_cue_text() {
        let cue = ExportCue {
            index: 1,
            start_sample: 0,
            end_sample: 1_000,
            sample_rate: 1_000,
            text: "第一部分，第二部分。".into(),
        };

        let output = render_srt(std::slice::from_ref(&cue));

        assert!(output.contains("第一部分  第二部分"));
        assert_eq!(cue.text, "第一部分，第二部分。");
    }

    #[test]
    fn renders_long_export_text_as_two_subtitle_lines() {
        let cue = ExportCue {
            index: 1,
            start_sample: 0,
            end_sample: 1_000,
            sample_rate: 1_000,
            text: "一二三四五六七八九十甲乙丙丁戊己庚辛壬癸子丑寅卯辰巳午。".into(),
        };

        assert_eq!(
            render_srt(&[cue]),
            "1\n00:00:00,000 --> 00:00:01,000\n\
             一二三四五六七八九十甲乙丙丁\n\
             戊己庚辛壬癸子丑寅卯辰巳午\n\n"
        );
    }
}
