"""Media inspection and audio decoding with PyAV."""

from pathlib import Path

import av
import numpy as np

from .errors import MediaError
from .types import AudioData, MediaKind

TARGET_SAMPLE_RATE = 16_000


def decode_audio(path: Path, sample_rate: int = TARGET_SAMPLE_RATE) -> AudioData:
    """Decode the first audio stream to mono float32 PCM."""
    if not path.is_file():
        raise MediaError(f"输入文件不存在：{path}")

    try:
        with av.open(str(path)) as container:
            audio_streams = container.streams.audio
            if not audio_streams:
                raise MediaError(f"文件中没有音轨：{path}")

            media_kind = (
                MediaKind.VIDEO if len(container.streams.video) > 0 else MediaKind.AUDIO
            )
            stream = audio_streams[0]
            resampler = av.AudioResampler(
                format="fltp",
                layout="mono",
                rate=sample_rate,
            )
            chunks: list[np.ndarray] = []

            for frame in container.decode(stream):
                resampled_frames = resampler.resample(frame)
                for resampled in resampled_frames:
                    chunks.append(_frame_to_mono(resampled))

            for resampled in resampler.resample(None):
                chunks.append(_frame_to_mono(resampled))
    except MediaError:
        raise
    except (av.error.FFmpegError, OSError, ValueError) as exc:
        raise MediaError(f"无法读取媒体文件 {path}：{exc}") from exc

    if not chunks:
        raise MediaError(f"音轨中没有可解码的音频：{path}")

    samples = np.ascontiguousarray(np.concatenate(chunks), dtype=np.float32)
    return AudioData(samples=samples, sample_rate=sample_rate, media_kind=media_kind)


def _frame_to_mono(frame: av.AudioFrame) -> np.ndarray:
    samples = frame.to_ndarray()
    if samples.ndim == 2:
        samples = samples[0]
    return np.asarray(samples, dtype=np.float32)
