"""Command-line interface."""

import argparse
import sys
from pathlib import Path

from .asr import SenseVoiceRecognizer
from .errors import AutoCutError
from .pipeline import run_pipeline
from .vad import SileroVoiceDetector

DEFAULT_VAD_MODEL = Path("models/silero_vad.onnx")
DEFAULT_ASR_MODEL = Path("models/sherpa-onnx-sense-voice-small/model.onnx")
DEFAULT_TOKENS = Path("models/sherpa-onnx-sense-voice-small/tokens.txt")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="aroll-autocut",
        description="使用 Silero VAD 和 SenseVoice 从音视频生成 SRT 字幕。",
    )
    parser.add_argument("input", type=Path, help="输入音频或视频文件")
    parser.add_argument("-o", "--output", type=Path, help="输出 SRT 文件路径")
    parser.add_argument("--vad-model", type=Path, default=DEFAULT_VAD_MODEL)
    parser.add_argument("--asr-model", type=Path, default=DEFAULT_ASR_MODEL)
    parser.add_argument("--tokens", type=Path, default=DEFAULT_TOKENS)
    parser.add_argument(
        "--language",
        choices=("auto", "zh", "en", "ja", "ko", "yue"),
        default="auto",
        help="识别语言（默认：auto）",
    )
    parser.add_argument(
        "--provider",
        choices=("cpu", "coreml", "cuda"),
        default="cpu",
        help="ONNX Runtime 执行后端（默认：cpu）",
    )
    parser.add_argument(
        "--num-threads",
        type=_positive_int,
        default=2,
        help="模型推理线程数（默认：2）",
    )
    parser.add_argument("--force", action="store_true", help="覆盖已有输出文件")
    parser.add_argument("--debug", action="store_true", help="启用模型调试日志")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    output = args.output or args.input.with_suffix(".srt")

    if output.exists() and not args.force:
        print(f"错误：输出文件已存在：{output}（使用 --force 覆盖）", file=sys.stderr)
        return 2

    try:
        detector = SileroVoiceDetector(
            args.vad_model,
            num_threads=args.num_threads,
            provider=args.provider,
            debug=args.debug,
        )
        recognizer = SenseVoiceRecognizer(
            args.asr_model,
            args.tokens,
            language=args.language,
            num_threads=args.num_threads,
            provider=args.provider,
            debug=args.debug,
        )
        media_kind, cue_count = run_pipeline(
            args.input,
            output,
            detector,
            recognizer,
            overwrite=args.force,
        )
    except (AutoCutError, FileExistsError) as exc:
        print(f"错误：{exc}", file=sys.stderr)
        return 1

    print(f"完成：{media_kind} 输入，生成 {cue_count} 条字幕 -> {output}")
    return 0


def _positive_int(value: str) -> int:
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("必须是大于 0 的整数")
    return number


if __name__ == "__main__":
    raise SystemExit(main())
