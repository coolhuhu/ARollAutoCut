# ARollCut

ARollCut 是一个基于字幕编辑口播音视频的桌面应用。应用使用 Tauri 2、React、TypeScript、Vite 和 Rust 构建，首个目标平台为 macOS Apple Silicon。

## 首版流程

```text
导入音频或视频
→ Silero VAD 检测
→ SenseVoice 转写
→ 修改、删除或恢复字幕块
→ 导出重新拼接的媒体和 SRT
```

Silero VAD 随 App 分发。SenseVoice 使用固定的 2024-07-17 INT8 模型，并由用户在 App 中下载或选择已有目录。

## 开发环境

- Node.js 22+
- npm 10+
- Rust stable
- macOS Apple Silicon

### sherpa-onnx 本地运行库

Rust crate 会使用 sherpa-onnx 官方 Apple Silicon 静态库。将以下文件放入 `.cache/sherpa-onnx/`：

```text
sherpa-onnx-v1.13.2-osx-arm64-static-lib.tar.bz2
```

SHA-256：

```text
e2d704b01c392970ee7fb90d7e74fd854528a172de0381228e987b79ac479f8e
```

### FFmpeg / FFprobe sidecar

ARollCut 固定使用 FFmpeg 8.1.1。将官方源码包放入：

```text
.cache/ffmpeg/ffmpeg-8.1.1.tar.xz
```

SHA-256：

```text
b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3
```

macOS Apple Silicon 构建需要 Homebrew LAME 3.100 静态库：

```bash
brew install lame
./scripts/build-ffmpeg-macos.sh
```

脚本会生成 Tauri sidecar：

```text
src-tauri/binaries/ffmpeg-aarch64-apple-darwin
src-tauri/binaries/ffprobe-aarch64-apple-darwin
```

当前 macOS Apple Silicon sidecar 的 SHA-256：

```text
ffmpeg:  b7f8243043f73401acb928c2fac9d6f20f123853ff21b3bd2019fdc87de2835d
ffprobe: 29435b07c07c0f8ea53666c96a5db5dcbcc3053e77fd6125e5396501b6a9a4ad
```

安装依赖：

```bash
npm install
```

启动 Web 前端：

```bash
npm run dev
```

启动 Tauri：

```bash
npm run tauri dev
```

如果当前网络无法直接访问 crates.io，可显式使用仓库提供的可选 runner：

```bash
npm run tauri -- build \
  --runner /absolute/path/to/ARollAutoCut/scripts/cargo-rsproxy.sh
```

## 测试

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

真实模型集成测试默认忽略，需要本地 `models/` 目录中的模型和测试音频。

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  --test speech_integration -- --ignored
```
