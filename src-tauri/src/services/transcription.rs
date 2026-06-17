use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::domain::transcript::TranscriptSegment;

use super::media_tools::{extract_speech_wave, MediaToolError};
use super::model_manager::VadSettings;
use super::speech::{SpeechEngine, SpeechError, SpeechModelPaths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionResult {
    pub source_path: PathBuf,
    pub source_name: String,
    pub media_kind: MediaKind,
    pub preview_audio_path: PathBuf,
    pub segments: Vec<TranscriptSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionProgress {
    pub stage: &'static str,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug)]
pub enum TranscriptionError {
    SourceNotFound(PathBuf),
    UnsupportedFormat(String),
    MissingFfmpeg,
    NoSpeech,
    Media(MediaToolError),
    PreviewAudio(std::io::Error),
    Speech(SpeechError),
}

impl fmt::Display for TranscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound(path) => write!(formatter, "媒体文件不存在：{}", path.display()),
            Self::UnsupportedFormat(extension) => {
                write!(formatter, "暂不支持此文件格式：{extension}")
            }
            Self::MissingFfmpeg => write!(formatter, "此文件需要 FFmpeg sidecar，但当前尚未安装"),
            Self::NoSpeech => write!(formatter, "没有检测到有效语音片段"),
            Self::Media(error) => error.fmt(formatter),
            Self::PreviewAudio(error) => write!(formatter, "无法准备预览音频：{error}"),
            Self::Speech(error) => error.fmt(formatter),
        }
    }
}

impl Error for TranscriptionError {}

pub fn classify_media(path: &Path) -> Result<MediaKind, TranscriptionError> {
    if !path.is_file() {
        return Err(TranscriptionError::SourceNotFound(path.into()));
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        "mp4" | "mov" => Ok(MediaKind::Video),
        "wav" | "mp3" | "m4a" | "aac" | "flac" => Ok(MediaKind::Audio),
        _ => Err(TranscriptionError::UnsupportedFormat(
            if extension.is_empty() {
                "无扩展名".into()
            } else {
                extension
            },
        )),
    }
}

pub fn transcribe_media(
    source: &Path,
    model_paths: SpeechModelPaths,
    vad_settings: VadSettings,
    preview_audio: &Path,
    ffmpeg: Option<&Path>,
    cancelled: &AtomicBool,
    mut on_progress: impl FnMut(TranscriptionProgress),
) -> Result<TranscriptionResult, TranscriptionError> {
    let media_kind = classify_media(source)?;
    check_cancelled(cancelled)?;
    on_progress(progress("preparing", 8, "正在加载语音识别模型"));

    let engine =
        SpeechEngine::create(model_paths, vad_settings).map_err(TranscriptionError::Speech)?;
    let is_cancelled = || cancelled.load(Ordering::Relaxed);

    on_progress(progress("vad", 35, "正在检测有效语音片段"));
    let direct_wave = source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("wav"));

    let segments = if direct_wave {
        fs::copy(source, preview_audio).map_err(TranscriptionError::PreviewAudio)?;
        match recognize_wave(&engine, preview_audio, &is_cancelled, &mut on_progress) {
            Ok(segments) => segments,
            Err(SpeechError::InvalidWave(_) | SpeechError::UnsupportedSampleRate(_))
                if ffmpeg.is_some() =>
            {
                transcribe_prepared_wave(
                    &engine,
                    source,
                    preview_audio,
                    ffmpeg.expect("checked above"),
                    &is_cancelled,
                    &mut on_progress,
                )?
            }
            Err(error) => return Err(TranscriptionError::Speech(error)),
        }
    } else {
        transcribe_prepared_wave(
            &engine,
            source,
            preview_audio,
            ffmpeg.ok_or(TranscriptionError::MissingFfmpeg)?,
            &is_cancelled,
            &mut on_progress,
        )?
    };

    if segments.is_empty() {
        return Err(TranscriptionError::NoSpeech);
    }
    check_cancelled(cancelled)?;
    on_progress(progress("complete", 100, "字幕生成完成"));

    Ok(TranscriptionResult {
        source_path: source.into(),
        source_name: source
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("未命名媒体")
            .to_owned(),
        media_kind,
        preview_audio_path: preview_audio.into(),
        segments,
    })
}

fn transcribe_prepared_wave(
    engine: &SpeechEngine,
    source: &Path,
    wave_path: &Path,
    ffmpeg: &Path,
    is_cancelled: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(TranscriptionProgress),
) -> Result<Vec<TranscriptSegment>, TranscriptionError> {
    on_progress(progress("preparing", 18, "正在提取并转换音轨"));
    extract_speech_wave(ffmpeg, source, wave_path, is_cancelled)
        .map_err(TranscriptionError::Media)?;
    on_progress(progress("vad", 35, "正在检测有效语音片段"));
    recognize_wave(engine, &wave_path, is_cancelled, on_progress)
        .map_err(TranscriptionError::Speech)
}

fn recognize_wave(
    engine: &SpeechEngine,
    wave_path: &Path,
    is_cancelled: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(TranscriptionProgress),
) -> Result<Vec<TranscriptSegment>, SpeechError> {
    let mut report_segment = |completed: usize, total: usize| {
        let ratio = if total == 0 {
            1.0
        } else {
            completed as f32 / total as f32
        };
        on_progress(progress(
            "recognizing",
            42 + (ratio * 53.0).round() as u8,
            &format!("正在识别第 {completed}/{total} 个语音片段"),
        ));
    };
    engine.transcribe_wave_with_progress(wave_path, is_cancelled, &mut report_segment)
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), TranscriptionError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(TranscriptionError::Speech(SpeechError::Cancelled))
    } else {
        Ok(())
    }
}

fn progress(stage: &'static str, percent: u8, message: &str) -> TranscriptionProgress {
    TranscriptionProgress {
        stage,
        percent,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{classify_media, MediaKind, TranscriptionError};

    #[test]
    fn classifies_confirmed_audio_and_video_formats_case_insensitively() {
        let directory = tempdir().expect("temp directory");
        for (name, expected) in [
            ("clip.MP4", MediaKind::Video),
            ("clip.mov", MediaKind::Video),
            ("voice.WAV", MediaKind::Audio),
            ("voice.mp3", MediaKind::Audio),
            ("voice.m4a", MediaKind::Audio),
            ("voice.aac", MediaKind::Audio),
            ("voice.flac", MediaKind::Audio),
        ] {
            let path = directory.path().join(name);
            fs::write(&path, b"media").expect("write media");
            assert_eq!(classify_media(&path).expect("classification"), expected);
        }
    }

    #[test]
    fn rejects_unconfirmed_media_formats() {
        let directory = tempdir().expect("temp directory");
        let path = directory.path().join("voice.ogg");
        fs::write(&path, b"media").expect("write media");

        assert!(matches!(
            classify_media(&path),
            Err(TranscriptionError::UnsupportedFormat(extension)) if extension == "ogg"
        ));
    }

    #[test]
    fn reports_a_missing_source_before_format_validation() {
        let path = Path::new("/missing/voice.wav");

        assert!(matches!(
            classify_media(path),
            Err(TranscriptionError::SourceNotFound(_))
        ));
    }

    use std::path::Path;
}
