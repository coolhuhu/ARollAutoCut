"""Voice activity detection using sherpa-onnx Silero VAD."""

from pathlib import Path

import numpy as np
import sherpa_onnx

from .errors import ModelError
from .types import SpeechSegment


class SileroVoiceDetector:
    def __init__(
        self,
        model_path: Path,
        *,
        sample_rate: int = 16_000,
        threshold: float = 0.5,
        min_silence_duration: float = 0.5,
        min_speech_duration: float = 0.25,
        max_speech_duration: float = 20.0,
        num_threads: int = 1,
        provider: str = "cpu",
        debug: bool = False,
    ) -> None:
        if not model_path.is_file():
            raise ModelError(f"Silero VAD 模型不存在：{model_path}")

        self.sample_rate = sample_rate
        self.window_size = 512
        try:
            silero_config = sherpa_onnx.SileroVadModelConfig(
                model=str(model_path),
                threshold=threshold,
                min_silence_duration=min_silence_duration,
                min_speech_duration=min_speech_duration,
                window_size=self.window_size,
                max_speech_duration=max_speech_duration,
            )
            config = sherpa_onnx.VadModelConfig(
                silero_vad=silero_config,
                sample_rate=sample_rate,
                num_threads=num_threads,
                provider=provider,
                debug=debug,
            )
            self._detector = sherpa_onnx.VoiceActivityDetector(
                config,
                buffer_size_in_seconds=max(60.0, max_speech_duration * 2),
            )
        except (RuntimeError, ValueError) as exc:
            raise ModelError(f"无法加载 Silero VAD 模型：{exc}") from exc

    def detect(self, samples: np.ndarray) -> list[SpeechSegment]:
        """Return completed speech segments with absolute sample positions."""
        segments: list[SpeechSegment] = []
        for offset in range(0, len(samples), self.window_size):
            chunk = samples[offset : offset + self.window_size]
            self._detector.accept_waveform(chunk)
            self._drain(segments)

        self._detector.flush()
        self._drain(segments)
        return segments

    def _drain(self, output: list[SpeechSegment]) -> None:
        while not self._detector.empty():
            segment = self._detector.front
            output.append(
                SpeechSegment(
                    samples=np.asarray(segment.samples, dtype=np.float32).copy(),
                    start_sample=int(segment.start),
                    sample_rate=self.sample_rate,
                )
            )
            self._detector.pop()
