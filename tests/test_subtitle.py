from aroll_autocut.subtitle import format_srt_timestamp, render_srt
from aroll_autocut.types import SubtitleCue


def test_format_srt_timestamp() -> None:
    assert format_srt_timestamp(0) == "00:00:00,000"
    assert format_srt_timestamp(65.432) == "00:01:05,432"
    assert format_srt_timestamp(3661.001) == "01:01:01,001"


def test_render_srt() -> None:
    cues = [
        SubtitleCue(0.5, 1.75, "第一句"),
        SubtitleCue(2.0, 3.0, "second line"),
    ]

    assert render_srt(cues) == (
        "1\n"
        "00:00:00,500 --> 00:00:01,750\n"
        "第一句\n\n"
        "2\n"
        "00:00:02,000 --> 00:00:03,000\n"
        "second line\n"
    )
