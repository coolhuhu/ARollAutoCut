from pathlib import Path

import pytest

from aroll_autocut.asr import SenseVoiceRecognizer
from aroll_autocut.media import decode_audio
from aroll_autocut.pipeline import transcribe_segments
from aroll_autocut.vad import SileroVoiceDetector

ROOT = Path(__file__).resolve().parents[1]
TEST_AUDIO = ROOT / "models/vad-test.wav"
VAD_MODEL = ROOT / "models/silero_vad.onnx"
ASR_MODEL = ROOT / "models/sherpa-onnx-sense-voice-small/model.onnx"
TOKENS = ROOT / "models/sherpa-onnx-sense-voice-small/tokens.txt"


@pytest.mark.integration
def test_vad_test_audio_end_to_end() -> None:
    required_files = (TEST_AUDIO, VAD_MODEL, ASR_MODEL, TOKENS)
    if not all(path.is_file() for path in required_files):
        pytest.skip("本地测试音频或模型文件不完整")

    audio = decode_audio(TEST_AUDIO)
    detector = SileroVoiceDetector(VAD_MODEL)
    recognizer = SenseVoiceRecognizer(ASR_MODEL, TOKENS)

    cues = transcribe_segments(audio.samples, detector, recognizer)

    assert cues
    assert all(cue.text.strip() for cue in cues)
    assert all(cue.end_seconds > cue.start_seconds for cue in cues)
    assert all(left.end_seconds <= right.start_seconds for left, right in zip(cues, cues[1:]))
