use std::error::Error;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::domain::export::{build_export_timeline, ExportTimeline, ExportTimelineError};
use crate::domain::transcript::TranscriptSegment;

use super::subtitle::render_srt;
use super::transcription::MediaKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub media_path: PathBuf,
    pub subtitle_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub percent: u8,
    pub message: String,
}

#[derive(Debug)]
pub enum ExportError {
    SourceNotFound(PathBuf),
    InvalidDestination(PathBuf),
    ContainerMismatch { source: String, destination: String },
    MissingExecutable { name: &'static str, path: PathBuf },
    ProbeFailed(String),
    MissingStream(&'static str),
    UnsupportedCodec { stream: &'static str, codec: String },
    Timeline(ExportTimelineError),
    Io(std::io::Error),
    FfmpegFailed(String),
    Cancelled,
}

impl fmt::Display for ExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceNotFound(path) => write!(formatter, "原始媒体不存在：{}", path.display()),
            Self::InvalidDestination(path) => {
                write!(formatter, "导出路径无效：{}", path.display())
            }
            Self::ContainerMismatch {
                source,
                destination,
            } => write!(
                formatter,
                "导出格式必须与原文件一致：原文件为 {source}，目标为 {destination}"
            ),
            Self::MissingExecutable { name, path } => {
                write!(formatter, "{name} sidecar 不存在：{}", path.display())
            }
            Self::ProbeFailed(message) => write!(formatter, "无法读取媒体编码信息：{message}"),
            Self::MissingStream(stream) => write!(formatter, "原始媒体中缺少{stream}轨道"),
            Self::UnsupportedCodec { stream, codec } => {
                write!(formatter, "暂不支持保持{stream}编码：{codec}")
            }
            Self::Timeline(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "导出文件失败：{error}"),
            Self::FfmpegFailed(message) => write!(formatter, "FFmpeg 导出失败：{message}"),
            Self::Cancelled => write!(formatter, "导出已取消"),
        }
    }
}

impl Error for ExportError {}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: String,
    codec_name: String,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct PrimaryCodecs {
    video: Option<String>,
    audio: Option<String>,
}

pub struct ExportRequest<'a> {
    pub ffmpeg: &'a Path,
    pub ffprobe: &'a Path,
    pub source: &'a Path,
    pub destination: &'a Path,
    pub media_kind: MediaKind,
    pub segments: &'a [TranscriptSegment],
}

pub fn export_edited_media(
    request: ExportRequest<'_>,
    cancelled: &AtomicBool,
    mut on_progress: impl FnMut(ExportProgress),
) -> Result<ExportResult, ExportError> {
    validate_paths(
        request.ffmpeg,
        request.ffprobe,
        request.source,
        request.destination,
    )?;
    validate_container(request.source, request.destination)?;
    check_cancelled(cancelled)?;
    on_progress(export_progress(8, "正在分析原始媒体"));

    let codecs = probe_primary_codecs(request.ffprobe, request.source)?;
    let timeline = build_export_timeline(request.segments).map_err(ExportError::Timeline)?;
    let filter = build_filter_graph(&timeline, request.media_kind);
    let video_encoder = match request.media_kind {
        MediaKind::Video => Some(video_encoder(
            codecs
                .video
                .as_deref()
                .ok_or(ExportError::MissingStream("视频"))?,
        )?),
        MediaKind::Audio => None,
    };
    let audio_encoder = audio_encoder(
        codecs
            .audio
            .as_deref()
            .ok_or(ExportError::MissingStream("音频"))?,
    )?;

    let temporary_media = temporary_output_path(request.destination);
    let subtitle_path = request.destination.with_extension("srt");
    let temporary_subtitle = temporary_output_path(&subtitle_path);
    remove_if_exists(&temporary_media)?;
    remove_if_exists(&temporary_subtitle)?;
    on_progress(export_progress(18, "正在拼接保留片段"));

    let arguments = build_ffmpeg_arguments(
        request.source,
        &temporary_media,
        request.media_kind,
        &filter,
        video_encoder,
        audio_encoder,
    );
    if let Err(error) = run_ffmpeg(request.ffmpeg, &arguments, cancelled) {
        let _ = fs::remove_file(&temporary_media);
        return Err(error);
    }

    on_progress(export_progress(92, "正在生成字幕文件"));
    if let Err(error) = fs::write(&temporary_subtitle, render_srt(&timeline.cues)) {
        let _ = fs::remove_file(&temporary_media);
        return Err(ExportError::Io(error));
    }
    replace_file(&temporary_media, request.destination)?;
    replace_file(&temporary_subtitle, &subtitle_path)?;
    on_progress(export_progress(100, "导出完成"));

    Ok(ExportResult {
        media_path: request.destination.into(),
        subtitle_path,
    })
}

fn validate_paths(
    ffmpeg: &Path,
    ffprobe: &Path,
    source: &Path,
    destination: &Path,
) -> Result<(), ExportError> {
    for (name, path) in [("ffmpeg", ffmpeg), ("ffprobe", ffprobe)] {
        if !path.is_file() {
            return Err(ExportError::MissingExecutable {
                name,
                path: path.into(),
            });
        }
    }
    if !source.is_file() {
        return Err(ExportError::SourceNotFound(source.into()));
    }
    if destination.file_name().is_none() || destination.parent().is_none() {
        return Err(ExportError::InvalidDestination(destination.into()));
    }
    Ok(())
}

fn validate_container(source: &Path, destination: &Path) -> Result<(), ExportError> {
    let source_extension = extension(source);
    let destination_extension = extension(destination);
    if source_extension != destination_extension {
        return Err(ExportError::ContainerMismatch {
            source: source_extension,
            destination: destination_extension,
        });
    }
    Ok(())
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or("无扩展名")
        .to_ascii_lowercase()
}

fn probe_primary_codecs(ffprobe: &Path, source: &Path) -> Result<PrimaryCodecs, ExportError> {
    let output = Command::new(ffprobe)
        .arg("-v")
        .arg("error")
        .arg("-show_entries")
        .arg("stream=codec_type,codec_name")
        .arg("-of")
        .arg("json")
        .arg(source)
        .output()
        .map_err(ExportError::Io)?;
    if !output.status.success() {
        return Err(ExportError::ProbeFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    let probe: ProbeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|error| ExportError::ProbeFailed(error.to_string()))?;
    let mut codecs = PrimaryCodecs::default();
    for stream in probe.streams {
        match stream.codec_type.as_str() {
            "video" if codecs.video.is_none() => codecs.video = Some(stream.codec_name),
            "audio" if codecs.audio.is_none() => codecs.audio = Some(stream.codec_name),
            _ => {}
        }
    }
    Ok(codecs)
}

fn video_encoder(codec: &str) -> Result<&'static str, ExportError> {
    match codec {
        "h264" => Ok(if cfg!(target_os = "macos") {
            "h264_videotoolbox"
        } else {
            "libx264"
        }),
        "hevc" => Ok(if cfg!(target_os = "macos") {
            "hevc_videotoolbox"
        } else {
            "libx265"
        }),
        "prores" => Ok("prores_ks"),
        _ => Err(ExportError::UnsupportedCodec {
            stream: "视频",
            codec: codec.to_owned(),
        }),
    }
}

fn audio_encoder(codec: &str) -> Result<&'static str, ExportError> {
    match codec {
        "aac" => Ok("aac"),
        "alac" => Ok("alac"),
        "flac" => Ok("flac"),
        "mp3" => Ok("libmp3lame"),
        "pcm_s16le" => Ok("pcm_s16le"),
        "pcm_s24le" => Ok("pcm_s24le"),
        "pcm_s32le" => Ok("pcm_s32le"),
        "pcm_f32le" => Ok("pcm_f32le"),
        _ => Err(ExportError::UnsupportedCodec {
            stream: "音频",
            codec: codec.to_owned(),
        }),
    }
}

fn build_filter_graph(timeline: &ExportTimeline, media_kind: MediaKind) -> String {
    let mut filters = Vec::new();
    for (index, range) in timeline.ranges.iter().enumerate() {
        let start = range.start_sample as f64 / timeline.sample_rate as f64;
        let end = range.end_sample as f64 / timeline.sample_rate as f64;
        if media_kind == MediaKind::Video {
            filters.push(format!(
                "[0:v:0]trim=start={start:.6}:end={end:.6},setpts=PTS-STARTPTS[v{index}]"
            ));
        }
        filters.push(format!(
            "[0:a:0]atrim=start={start:.6}:end={end:.6},asetpts=PTS-STARTPTS[a{index}]"
        ));
    }

    let inputs = (0..timeline.ranges.len())
        .map(|index| match media_kind {
            MediaKind::Video => format!("[v{index}][a{index}]"),
            MediaKind::Audio => format!("[a{index}]"),
        })
        .collect::<String>();
    filters.push(match media_kind {
        MediaKind::Video => format!(
            "{inputs}concat=n={}:v=1:a=1[outv][outa]",
            timeline.ranges.len()
        ),
        MediaKind::Audio => format!("{inputs}concat=n={}:v=0:a=1[outa]", timeline.ranges.len()),
    });
    filters.join(";")
}

fn build_ffmpeg_arguments(
    source: &Path,
    destination: &Path,
    media_kind: MediaKind,
    filter: &str,
    video_encoder: Option<&str>,
    audio_encoder: &str,
) -> Vec<OsString> {
    let mut arguments = vec![
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-y".into(),
        "-i".into(),
        source.as_os_str().to_owned(),
        "-filter_complex".into(),
        filter.into(),
    ];
    if media_kind == MediaKind::Video {
        arguments.extend([
            "-map".into(),
            "[outv]".into(),
            "-map".into(),
            "[outa]".into(),
            "-c:v".into(),
            video_encoder.expect("video export requires encoder").into(),
        ]);
    } else {
        arguments.extend(["-map".into(), "[outa]".into()]);
    }
    arguments.extend([
        "-c:a".into(),
        audio_encoder.into(),
        destination.as_os_str().to_owned(),
    ]);
    arguments
}

fn run_ffmpeg(
    ffmpeg: &Path,
    arguments: &[OsString],
    cancelled: &AtomicBool,
) -> Result<(), ExportError> {
    let mut child = Command::new(ffmpeg)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ExportError::Io)?;

    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportError::Cancelled);
        }
        match child.try_wait().map_err(ExportError::Io)? {
            Some(status) => {
                let output = child.wait_with_output().map_err(ExportError::Io)?;
                if status.success() {
                    return Ok(());
                }
                return Err(ExportError::FfmpegFailed(
                    String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                ));
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn temporary_output_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    path.with_file_name(format!(".{stem}.arollcut.tmp.{extension}"))
}

fn replace_file(temporary: &Path, destination: &Path) -> Result<(), ExportError> {
    remove_if_exists(destination)?;
    fs::rename(temporary, destination).map_err(ExportError::Io)
}

fn remove_if_exists(path: &Path) -> Result<(), ExportError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ExportError::Io(error)),
    }
}

fn check_cancelled(cancelled: &AtomicBool) -> Result<(), ExportError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ExportError::Cancelled)
    } else {
        Ok(())
    }
}

fn export_progress(percent: u8, message: &str) -> ExportProgress {
    ExportProgress {
        percent,
        message: message.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        audio_encoder, build_filter_graph, temporary_output_path, validate_container,
        video_encoder, ExportError,
    };
    use crate::domain::export::build_export_timeline;
    use crate::domain::transcript::TranscriptSegment;
    use crate::services::transcription::MediaKind;

    #[test]
    fn builds_audio_filters_from_merged_vad_ranges() {
        let timeline =
            build_export_timeline(&[segment(1, 16_000, 32_000), segment(2, 48_000, 64_000)])
                .expect("timeline");

        let filter = build_filter_graph(&timeline, MediaKind::Audio);

        assert_eq!(
            filter,
            "[0:a:0]atrim=start=1.000000:end=2.000000,asetpts=PTS-STARTPTS[a0];\
             [0:a:0]atrim=start=3.000000:end=4.000000,asetpts=PTS-STARTPTS[a1];\
             [a0][a1]concat=n=2:v=0:a=1[outa]"
        );
    }

    #[test]
    fn builds_synchronized_video_and_audio_filters() {
        let timeline = build_export_timeline(&[segment(1, 8_000, 24_000)]).expect("timeline");

        let filter = build_filter_graph(&timeline, MediaKind::Video);

        assert!(filter.contains("[0:v:0]trim=start=0.500000:end=1.500000"));
        assert!(filter.contains("[0:a:0]atrim=start=0.500000:end=1.500000"));
        assert!(filter.ends_with("[v0][a0]concat=n=1:v=1:a=1[outv][outa]"));
    }

    #[test]
    fn maps_confirmed_source_codecs_to_matching_encoders() {
        assert!(video_encoder("h264")
            .expect("h264 encoder")
            .contains("h264"));
        assert!(video_encoder("hevc")
            .expect("hevc encoder")
            .contains("hevc"));
        assert_eq!(
            video_encoder("prores").expect("prores encoder"),
            "prores_ks"
        );
        assert_eq!(audio_encoder("aac").expect("aac encoder"), "aac");
        assert_eq!(audio_encoder("mp3").expect("mp3 encoder"), "libmp3lame");
        assert!(matches!(
            audio_encoder("opus"),
            Err(ExportError::UnsupportedCodec { .. })
        ));
    }

    #[test]
    fn requires_the_destination_container_to_match_the_source() {
        assert!(validate_container(Path::new("source.MOV"), Path::new("output.mov")).is_ok());
        assert!(matches!(
            validate_container(Path::new("source.mov"), Path::new("output.mp4")),
            Err(ExportError::ContainerMismatch { .. })
        ));
    }

    #[test]
    fn keeps_the_real_container_extension_on_temporary_outputs() {
        assert_eq!(
            temporary_output_path(Path::new("/tmp/final cut.mp4")),
            Path::new("/tmp/.final cut.arollcut.tmp.mp4")
        );
    }

    fn segment(id: u32, start: u64, end: u64) -> TranscriptSegment {
        TranscriptSegment::new(id, start, end, 16_000, format!("第{id}句"))
    }
}
