use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use sherpa_onnx::{
    OfflineRecognizer, OfflineRecognizerConfig, OfflineSenseVoiceModelConfig, SileroVadModelConfig,
    VadModelConfig, VoiceActivityDetector, Wave,
};

use crate::domain::subtitle_split::split_recognition;
use crate::domain::transcript::TranscriptSegment;

use super::model_manager::VadSettings;

const SAMPLE_RATE: i32 = 16_000;
const VAD_WINDOW_SIZE: usize = 512;

#[derive(Debug, Clone)]
pub struct SpeechModelPaths {
    pub vad_model: PathBuf,
    pub sense_voice_model: PathBuf,
    pub tokens: PathBuf,
}

impl SpeechModelPaths {
    pub fn validate(&self) -> Result<(), SpeechError> {
        for (label, path) in [
            ("Silero VAD 模型", &self.vad_model),
            ("SenseVoice 模型", &self.sense_voice_model),
            ("SenseVoice tokens", &self.tokens),
        ] {
            if !path.is_file() {
                return Err(SpeechError::MissingModel {
                    label,
                    path: path.clone(),
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum SpeechError {
    MissingModel { label: &'static str, path: PathBuf },
    InvalidWave(PathBuf),
    UnsupportedSampleRate(i32),
    VadInitialization,
    RecognizerInitialization,
    MissingRecognitionResult,
    Cancelled,
}

impl fmt::Display for SpeechError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingModel { label, path } => {
                write!(formatter, "{label}不存在：{}", path.display())
            }
            Self::InvalidWave(path) => write!(formatter, "无法读取 WAV 文件：{}", path.display()),
            Self::UnsupportedSampleRate(rate) => {
                write!(formatter, "技术验证仅支持 16000 Hz WAV，实际为 {rate} Hz")
            }
            Self::VadInitialization => write!(formatter, "无法初始化 Silero VAD"),
            Self::RecognizerInitialization => write!(formatter, "无法初始化 SenseVoice"),
            Self::MissingRecognitionResult => write!(formatter, "SenseVoice 未返回识别结果"),
            Self::Cancelled => write!(formatter, "识别已取消"),
        }
    }
}

impl Error for SpeechError {}

#[derive(Debug)]
struct DetectedSpeech {
    start_sample: u64,
    samples: Vec<f32>,
}

pub struct SpeechEngine {
    paths: SpeechModelPaths,
    vad_settings: VadSettings,
    recognizer: OfflineRecognizer,
}

impl SpeechEngine {
    pub fn create(paths: SpeechModelPaths, vad_settings: VadSettings) -> Result<Self, SpeechError> {
        paths.validate()?;
        let vad_settings = vad_settings
            .validate()
            .map_err(|_| SpeechError::VadInitialization)?;

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
            model: Some(path_string(&paths.sense_voice_model)),
            language: Some("auto".into()),
            use_itn: true,
        };
        config.model_config.tokens = Some(path_string(&paths.tokens));
        config.model_config.num_threads = 2;
        config.model_config.provider = Some("cpu".into());

        let recognizer =
            OfflineRecognizer::create(&config).ok_or(SpeechError::RecognizerInitialization)?;

        Ok(Self {
            paths,
            vad_settings,
            recognizer,
        })
    }

    pub fn transcribe_wave(&self, path: &Path) -> Result<Vec<TranscriptSegment>, SpeechError> {
        self.transcribe_wave_with_progress(path, &|| false, &mut |_, _| {})
    }

    pub fn transcribe_wave_with_progress(
        &self,
        path: &Path,
        is_cancelled: &dyn Fn() -> bool,
        on_segment: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<TranscriptSegment>, SpeechError> {
        if is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        let wave =
            Wave::read(&path_string(path)).ok_or_else(|| SpeechError::InvalidWave(path.into()))?;
        if wave.sample_rate() != SAMPLE_RATE {
            return Err(SpeechError::UnsupportedSampleRate(wave.sample_rate()));
        }

        self.transcribe_samples_with_progress(wave.samples(), is_cancelled, on_segment)
    }

    pub fn transcribe_samples(
        &self,
        samples: &[f32],
    ) -> Result<Vec<TranscriptSegment>, SpeechError> {
        self.transcribe_samples_with_progress(samples, &|| false, &mut |_, _| {})
    }

    pub fn transcribe_samples_with_progress(
        &self,
        samples: &[f32],
        is_cancelled: &dyn Fn() -> bool,
        on_segment: &mut dyn FnMut(usize, usize),
    ) -> Result<Vec<TranscriptSegment>, SpeechError> {
        if is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        let detected = detect_speech(samples, &self.paths.vad_model, self.vad_settings)?;
        let total = detected.len();
        let mut segments = Vec::new();
        let mut next_segment_id = 1;

        for (vad_index, speech) in detected.into_iter().enumerate() {
            if is_cancelled() {
                return Err(SpeechError::Cancelled);
            }
            let stream = self.recognizer.create_stream();
            stream.accept_waveform(SAMPLE_RATE, &speech.samples);
            self.recognizer.decode(&stream);
            let result = stream
                .get_result()
                .ok_or(SpeechError::MissingRecognitionResult)?;
            let end_sample = speech.start_sample + speech.samples.len() as u64;
            let subtitles = split_recognition(
                speech.start_sample,
                end_sample,
                SAMPLE_RATE as u32,
                &result.text,
                &result.tokens,
                result.timestamps.as_deref(),
            );
            for subtitle in subtitles {
                segments.push(TranscriptSegment::new(
                    next_segment_id,
                    subtitle.start_sample,
                    subtitle.end_sample,
                    SAMPLE_RATE as u32,
                    subtitle.text,
                ));
                next_segment_id += 1;
            }
            on_segment(vad_index + 1, total);
        }

        Ok(segments)
    }
}

fn detect_speech(
    samples: &[f32],
    model_path: &Path,
    vad_settings: VadSettings,
) -> Result<Vec<DetectedSpeech>, SpeechError> {
    let config = build_vad_config(model_path, vad_settings);
    let detector =
        VoiceActivityDetector::create(&config, 60.0).ok_or(SpeechError::VadInitialization)?;
    let mut output = Vec::new();

    for chunk in samples.chunks(VAD_WINDOW_SIZE) {
        detector.accept_waveform(chunk);
        drain_segments(&detector, &mut output);
    }
    detector.flush();
    drain_segments(&detector, &mut output);

    Ok(output)
}

fn build_vad_config(model_path: &Path, vad_settings: VadSettings) -> VadModelConfig {
    VadModelConfig {
        silero_vad: SileroVadModelConfig {
            model: Some(path_string(model_path)),
            threshold: 0.5,
            min_silence_duration: vad_settings.min_silence_duration,
            min_speech_duration: vad_settings.min_speech_duration,
            window_size: VAD_WINDOW_SIZE as i32,
            max_speech_duration: vad_settings.max_speech_duration,
        },
        sample_rate: SAMPLE_RATE,
        num_threads: 1,
        provider: Some("cpu".into()),
        debug: false,
        ..Default::default()
    }
}

fn drain_segments(detector: &VoiceActivityDetector, output: &mut Vec<DetectedSpeech>) {
    while let Some(segment) = detector.front() {
        output.push(DetectedSpeech {
            start_sample: segment.start() as u64,
            samples: segment.samples().to_vec(),
        });
        detector.pop();
    }
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{build_vad_config, SpeechError, SpeechModelPaths};
    use crate::services::model_manager::VadSettings;

    #[test]
    fn reports_the_first_missing_model_file() {
        let paths = SpeechModelPaths {
            vad_model: PathBuf::from("/missing/silero.onnx"),
            sense_voice_model: PathBuf::from("/missing/sense-voice.onnx"),
            tokens: PathBuf::from("/missing/tokens.txt"),
        };

        let error = paths.validate().expect_err("validation should fail");

        assert!(matches!(
            error,
            SpeechError::MissingModel {
                label: "Silero VAD 模型",
                ..
            }
        ));
    }

    #[test]
    fn builds_vad_config_from_user_settings() {
        let settings = VadSettings {
            min_silence_duration: 0.8,
            min_speech_duration: 0.4,
            max_speech_duration: 42.0,
        };

        let config = build_vad_config(PathBuf::from("/models/vad.onnx").as_path(), settings);

        assert_eq!(config.silero_vad.min_silence_duration, 0.8);
        assert_eq!(config.silero_vad.min_speech_duration, 0.4);
        assert_eq!(config.silero_vad.max_speech_duration, 42.0);
    }
}
