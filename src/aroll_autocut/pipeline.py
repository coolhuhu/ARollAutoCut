"""End-to-end transcription pipeline."""

from pathlib import Path
from typing import Protocol

import numpy as np

from .media import decode_audio
from .subtitle import write_srt
from .types import SpeechSegment, SubtitleCue


class VoiceDetector(Protocol):
    def detect(self, samples: np.ndarray) -> list[SpeechSegment]: ...


class SpeechRecognizer(Protocol):
    def transcribe(self, samples: np.ndarray, sample_rate: int) -> str: ...


def transcribe_segments(
    samples: np.ndarray,
    detector: VoiceDetector,
    recognizer: SpeechRecognizer,
) -> list[SubtitleCue]:
    cues = []
    for segment in detector.detect(samples):
        text = recognizer.transcribe(segment.samples, segment.sample_rate).strip()
        if text:
            cues.append(
                SubtitleCue(
                    start_seconds=segment.start_seconds,
                    end_seconds=segment.end_seconds,
                    text=text,
                )
            )
    return cues


def run_pipeline(
    input_path: Path,
    output_path: Path,
    detector: VoiceDetector,
    recognizer: SpeechRecognizer,
    *,
    overwrite: bool = False,
) -> tuple[str, int]:
    audio = decode_audio(input_path)
    cues = transcribe_segments(audio.samples, detector, recognizer)
    write_srt(output_path, cues, overwrite=overwrite)
    return audio.media_kind.value, len(cues)
