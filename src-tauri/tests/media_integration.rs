use std::fs;
use std::path::PathBuf;
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
