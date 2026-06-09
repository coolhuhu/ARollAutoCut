"""Speech recognition using sherpa-onnx SenseVoice."""

from pathlib import Path

import numpy as np
import sherpa_onnx

from .errors import ModelError


class SenseVoiceRecognizer:
    def __init__(
        self,
        model_path: Path,
        tokens_path: Path,
        *,
        language: str = "auto",
        use_itn: bool = True,
        num_threads: int = 2,
        provider: str = "cpu",
        debug: bool = False,
    ) -> None:
        for label, path in (("SenseVoice 模型", model_path), ("tokens 文件", tokens_path)):
            if not path.is_file():
                raise ModelError(f"{label}不存在：{path}")

        try:
            self._recognizer = sherpa_onnx.OfflineRecognizer.from_sense_voice(
                model=str(model_path),
                tokens=str(tokens_path),
                num_threads=num_threads,
                sample_rate=16_000,
                feature_dim=80,
                provider=provider,
                language=language,
                use_itn=use_itn,
                debug=debug,
            )
        except (RuntimeError, ValueError, AssertionError) as exc:
            raise ModelError(f"无法加载 SenseVoice 模型：{exc}") from exc

    def transcribe(self, samples: np.ndarray, sample_rate: int) -> str:
        try:
            stream = self._recognizer.create_stream()
            stream.accept_waveform(sample_rate, samples)
            self._recognizer.decode_stream(stream)
            return stream.result.text.strip()
        except (RuntimeError, ValueError) as exc:
            raise ModelError(f"语音识别失败：{exc}") from exc
