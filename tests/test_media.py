import wave
from pathlib import Path

import numpy as np
import pytest

from aroll_autocut.errors import MediaError
from aroll_autocut.media import decode_audio
from aroll_autocut.types import MediaKind


def test_decode_wav_as_mono_float32(tmp_path: Path) -> None:
    path = tmp_path / "stereo.wav"
    time = np.arange(800) / 8_000
    mono = (np.sin(2 * np.pi * 440 * time) * 10_000).astype("<i2")
    samples = np.column_stack((mono, mono))
    with wave.open(str(path), "wb") as output:
        output.setnchannels(2)
        output.setsampwidth(2)
        output.setframerate(8_000)
        output.writeframes(samples.tobytes())

    decoded = decode_audio(path)

    assert decoded.media_kind is MediaKind.AUDIO
    assert decoded.sample_rate == 16_000
    assert decoded.samples.dtype == np.float32
    assert decoded.samples.ndim == 1
    assert len(decoded.samples) > 0


def test_missing_input_raises_media_error(tmp_path: Path) -> None:
    with pytest.raises(MediaError, match="输入文件不存在"):
        decode_audio(tmp_path / "missing.wav")
