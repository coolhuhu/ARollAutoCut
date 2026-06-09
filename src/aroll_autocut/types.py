"""Shared data structures."""

from dataclasses import dataclass
from enum import Enum

import numpy as np


class MediaKind(str, Enum):
    AUDIO = "audio"
    VIDEO = "video"


@dataclass(frozen=True)
class AudioData:
    samples: np.ndarray
    sample_rate: int
    media_kind: MediaKind


@dataclass(frozen=True)
class SpeechSegment:
    samples: np.ndarray
    start_sample: int
    sample_rate: int

    @property
    def start_seconds(self) -> float:
        return self.start_sample / self.sample_rate

    @property
    def end_seconds(self) -> float:
        return (self.start_sample + len(self.samples)) / self.sample_rate


@dataclass(frozen=True)
class SubtitleCue:
    start_seconds: float
    end_seconds: float
    text: str
