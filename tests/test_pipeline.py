import numpy as np

from aroll_autocut.pipeline import transcribe_segments
from aroll_autocut.types import SpeechSegment


class FakeDetector:
    def detect(self, samples: np.ndarray) -> list[SpeechSegment]:
        return [
            SpeechSegment(np.ones(16_000, dtype=np.float32), 8_000, 16_000),
            SpeechSegment(np.ones(8_000, dtype=np.float32), 32_000, 16_000),
        ]


class FakeRecognizer:
    def __init__(self) -> None:
        self.calls = 0

    def transcribe(self, samples: np.ndarray, sample_rate: int) -> str:
        self.calls += 1
        return "有效字幕" if self.calls == 1 else " "


def test_transcribe_segments_uses_vad_timestamps_and_skips_empty_text() -> None:
    recognizer = FakeRecognizer()

    cues = transcribe_segments(
        np.zeros(48_000, dtype=np.float32),
        FakeDetector(),
        recognizer,
    )

    assert len(cues) == 1
    assert cues[0].start_seconds == 0.5
    assert cues[0].end_seconds == 1.5
    assert cues[0].text == "有效字幕"
    assert recognizer.calls == 2
