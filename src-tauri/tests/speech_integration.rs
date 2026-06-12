use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use arollcut_lib::services::speech::{SpeechEngine, SpeechModelPaths};
use arollcut_lib::services::transcription::{transcribe_media, MediaKind};

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

#[test]
#[ignore = "loads local Silero and SenseVoice models"]
fn transcribes_the_vad_test_audio_with_rust() {
    let engine = SpeechEngine::create(SpeechModelPaths {
        vad_model: repository_path("src-tauri/resources/silero_vad.onnx"),
        sense_voice_model: repository_path("models/sherpa-onnx-sense-voice-small/model.int8.onnx"),
        tokens: repository_path("models/sherpa-onnx-sense-voice-small/tokens.txt"),
    })
    .expect("create speech engine");

    let segments = engine
        .transcribe_wave(&repository_path("models/vad-test.wav"))
        .expect("transcribe test wave");

    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.edited_text.as_str())
            .collect::<Vec<_>>(),
        vec![
            "今天的天气很不错。",
            "我计划明天出去郊游。",
            "叫上我的好朋友们。"
        ]
    );
    assert!(segments
        .windows(2)
        .all(|pair| pair[0].end_sample <= pair[1].start_sample));
}

#[test]
#[ignore = "loads local Silero and SenseVoice models"]
fn runs_the_complete_wav_import_pipeline() {
    let source = repository_path("models/vad-test.wav");
    let cancelled = AtomicBool::new(false);
    let mut progress = Vec::new();

    let result = transcribe_media(
        &source,
        SpeechModelPaths {
            vad_model: repository_path("src-tauri/resources/silero_vad.onnx"),
            sense_voice_model: repository_path(
                "models/sherpa-onnx-sense-voice-small/model.int8.onnx",
            ),
            tokens: repository_path("models/sherpa-onnx-sense-voice-small/tokens.txt"),
        },
        None,
        &cancelled,
        |update| progress.push(update),
    )
    .expect("transcribe media");

    assert_eq!(result.media_kind, MediaKind::Audio);
    assert_eq!(result.source_name, "vad-test.wav");
    assert_eq!(result.segments.len(), 3);
    assert_eq!(progress.last().expect("final progress").percent, 100);
    assert!(progress.iter().any(|update| update.stage == "recognizing"));
}
