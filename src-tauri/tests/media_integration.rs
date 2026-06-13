use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;

use arollcut_lib::domain::transcript::TranscriptSegment;
use arollcut_lib::services::exporter::{export_edited_media, ExportRequest};
use arollcut_lib::services::transcription::MediaKind;
use tempfile::tempdir;

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

#[test]
#[ignore = "uses bundled FFmpeg sidecars and local test audio"]
fn exports_retained_audio_and_recalculates_srt_timestamps() {
    let output_directory = tempdir().expect("output directory");
    let output_path = output_directory.path().join("vad-test-cut.wav");
    let cancelled = AtomicBool::new(false);
    let mut deleted = TranscriptSegment::new(2, 16_000, 32_000, 16_000, "删除这一句".to_owned());
    deleted.delete();
    let segments = vec![
        TranscriptSegment::new(1, 0, 16_000, 16_000, "保留第一句".to_owned()),
        deleted,
        TranscriptSegment::new(3, 32_000, 48_000, 16_000, "保留第三句".to_owned()),
    ];

    let result = export_edited_media(
        ExportRequest {
            ffmpeg: &repository_path("src-tauri/binaries/ffmpeg-aarch64-apple-darwin"),
            ffprobe: &repository_path("src-tauri/binaries/ffprobe-aarch64-apple-darwin"),
            source: &repository_path("models/vad-test.wav"),
            destination: &output_path,
            media_kind: MediaKind::Audio,
            segments: &segments,
        },
        &cancelled,
        |_| {},
    )
    .expect("export edited audio");

    assert!(result.media_path.metadata().expect("output metadata").len() > 44);
    assert_eq!(
        fs::read_to_string(result.subtitle_path).expect("subtitle"),
        "1\n00:00:00,000 --> 00:00:01,000\n保留第一句\n\n\
         2\n00:00:01,000 --> 00:00:02,000\n保留第三句\n\n"
    );
}

#[test]
#[ignore = "uses bundled FFmpeg sidecars to generate and export a test video"]
fn preserves_primary_video_properties_when_exporting_a_mov() {
    let directory = tempdir().expect("output directory");
    let source = directory.path().join("source.mov");
    let output = directory.path().join("source-cut.mov");
    let ffmpeg = repository_path("src-tauri/binaries/ffmpeg-aarch64-apple-darwin");
    let ffprobe = repository_path("src-tauri/binaries/ffprobe-aarch64-apple-darwin");

    run(
        Command::new(&ffmpeg)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args([
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x180:rate=24:duration=3",
            ])
            .args([
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:sample_rate=48000:duration=3",
            ])
            .args(["-c:v", "prores_ks", "-profile:v", "0"])
            .args(["-pix_fmt", "yuv422p10le", "-c:a", "pcm_s16le"])
            .arg(&source),
        "generate test video",
    );

    let segments = vec![
        TranscriptSegment::new(1, 0, 12_000, 16_000, "保留第一段".to_owned()),
        TranscriptSegment::new(2, 24_000, 36_000, 16_000, "保留第二段".to_owned()),
    ];
    export_edited_media(
        ExportRequest {
            ffmpeg: &ffmpeg,
            ffprobe: &ffprobe,
            source: &source,
            destination: &output,
            media_kind: MediaKind::Video,
            segments: &segments,
        },
        &AtomicBool::new(false),
        |_| {},
    )
    .expect("export edited video");

    let source_probe = probe_streams(&ffprobe, &source);
    let output_probe = probe_streams(&ffprobe, &output);
    assert_eq!(
        primary_stream_properties(&source_probe, "video"),
        primary_stream_properties(&output_probe, "video")
    );
    assert_eq!(
        primary_stream_properties(&source_probe, "audio"),
        primary_stream_properties(&output_probe, "audio")
    );
    assert_eq!(
        fs::read_to_string(output.with_extension("srt")).expect("subtitle"),
        "1\n00:00:00,000 --> 00:00:00,750\n保留第一段\n\n\
         2\n00:00:00,750 --> 00:00:01,500\n保留第二段\n\n"
    );
}

fn probe_streams(ffprobe: &PathBuf, media: &std::path::Path) -> serde_json::Value {
    let output = Command::new(ffprobe)
        .args(["-v", "error"])
        .args([
            "-show_entries",
            "stream=codec_type,codec_name,width,height,r_frame_rate,sample_rate",
        ])
        .args(["-of", "json"])
        .arg(media)
        .output()
        .expect("run ffprobe");
    assert!(
        output.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse ffprobe output")
}

fn primary_stream_properties(probe: &serde_json::Value, kind: &str) -> serde_json::Value {
    probe["streams"]
        .as_array()
        .expect("streams")
        .iter()
        .find(|stream| stream["codec_type"] == kind)
        .expect("primary stream")
        .clone()
}

fn run(command: &mut Command, operation: &str) {
    let output = command.output().expect(operation);
    assert!(
        output.status.success(),
        "{operation} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
