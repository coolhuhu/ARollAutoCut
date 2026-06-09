"""SRT subtitle rendering."""

from pathlib import Path

from .types import SubtitleCue


def format_srt_timestamp(seconds: float) -> str:
    total_milliseconds = max(0, round(seconds * 1000))
    hours, remainder = divmod(total_milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    secs, milliseconds = divmod(remainder, 1000)
    return f"{hours:02d}:{minutes:02d}:{secs:02d},{milliseconds:03d}"


def render_srt(cues: list[SubtitleCue]) -> str:
    blocks = []
    for index, cue in enumerate(cues, start=1):
        blocks.append(
            "\n".join(
                (
                    str(index),
                    f"{format_srt_timestamp(cue.start_seconds)} --> "
                    f"{format_srt_timestamp(cue.end_seconds)}",
                    cue.text.strip(),
                )
            )
        )
    return "\n\n".join(blocks) + ("\n" if blocks else "")


def write_srt(path: Path, cues: list[SubtitleCue], *, overwrite: bool = False) -> None:
    if path.exists() and not overwrite:
        raise FileExistsError(f"输出文件已存在：{path}（使用 --force 覆盖）")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(render_srt(cues), encoding="utf-8")
