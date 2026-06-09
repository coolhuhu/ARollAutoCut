# ARollAutoCut

使用 sherpa-onnx、SenseVoice Small 和 Silero VAD，从音频或视频生成 SRT 字幕。

## 环境

项目使用 [uv](https://docs.astral.sh/uv/) 管理 Python 环境：

```bash
uv sync
```

默认模型目录：

```text
models/
├── silero_vad.onnx
└── sherpa-onnx-sense-voice-small/
    ├── model.onnx
    └── tokens.txt
```

## 使用

```bash
uv run aroll-autocut input.wav
uv run aroll-autocut input.mp4 --output subtitles.srt
```

输出文件默认与输入文件同目录、同名，扩展名为 `.srt`。

查看全部选项：

```bash
uv run aroll-autocut --help
```

## 测试

```bash
uv run pytest -m "not integration"
uv run pytest -m integration
```
