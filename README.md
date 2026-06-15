# ARollCut

ARollCut 是一款通过编辑字幕来剪辑口播音视频的桌面应用。

## 编译项目

当前可复现并已验证的构建平台为 macOS Apple Silicon，最低支持 macOS 12。

### 环境要求

- Apple Silicon Mac
- Xcode Command Line Tools
- Node.js 22+
- npm 10+
- Rust stable

安装 Xcode Command Line Tools：

```bash
xcode-select --install
```

安装 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

安装项目依赖：

```bash
npm ci
```

### sherpa-onnx 运行库

构建时，`sherpa-onnx` crate 会自动下载适用于 Apple Silicon 的官方静态运行库：

```text
sherpa-onnx-v1.13.2-osx-arm64-static-lib.tar.bz2
```

需要离线构建时，将该文件放入 `.cache/sherpa-onnx/`，然后设置：

```bash
export SHERPA_ONNX_ARCHIVE_DIR="$PWD/.cache/sherpa-onnx"
```

归档文件 SHA-256：

```text
e2d704b01c392970ee7fb90d7e74fd854528a172de0381228e987b79ac479f8e
```

### FFmpeg sidecar

仓库已包含 macOS Apple Silicon 使用的 FFmpeg 8.1.1 和 FFprobe sidecar：

```text
src-tauri/binaries/ffmpeg-aarch64-apple-darwin
src-tauri/binaries/ffprobe-aarch64-apple-darwin
```

如需重新构建 sidecar，先安装 LAME：

```bash
brew install lame
```

将 FFmpeg 8.1.1 源码包保存为：

```text
.cache/ffmpeg/ffmpeg-8.1.1.tar.xz
```

源码包 SHA-256：

```text
b6863adde98898f42602017462871b5f6333e65aec803fdd7a6308639c52edf3
```

然后执行：

```bash
./scripts/build-ffmpeg-macos.sh
```

### 启动开发版本

```bash
npm run tauri -- dev
```

### 编译发布版本

```bash
npm run tauri -- build
```

如果当前网络无法直接访问 crates.io，可以使用仓库提供的 Cargo runner：

```bash
npm run tauri -- build --runner "$PWD/scripts/cargo-rsproxy.sh"
```

构建成功后会生成：

```text
src-tauri/target/release/bundle/macos/ARollCut.app
src-tauri/target/release/bundle/dmg/ARollCut_<version>_aarch64.dmg
```

## 验证编译

依次运行前端测试、前端生产构建、Rust 测试和 Clippy：

```bash
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

执行完整 macOS 发布构建：

```bash
npm run tauri -- build
```

验证生成的 DMG：

```bash
hdiutil verify src-tauri/target/release/bundle/dmg/ARollCut_<version>_aarch64.dmg
```

真实语音和媒体集成测试需要以下本地文件：

```text
models/vad-test.wav
models/sherpa-onnx-sense-voice-small/model.int8.onnx
models/sherpa-onnx-sense-voice-small/tokens.txt
```

文件准备完成后运行：

```bash
cargo test --manifest-path src-tauri/Cargo.toml \
  --test speech_integration -- --ignored

cargo test --manifest-path src-tauri/Cargo.toml \
  --test media_integration -- --ignored
```

## 安装与使用

### 安装

1. 打开生成的 `ARollCut_<version>_aarch64.dmg`。
2. 将 `ARollCut.app` 拖入 macOS 的“应用程序”目录。
3. 当前开发版本未进行 Apple 公证。如果 macOS 阻止首次启动，请在 Finder 中右键点击 ARollCut，然后选择“打开”。

### 配置 SenseVoice

首次启动后，点击“模型设置”，选择以下任一方式：

1. 点击“下载模型”，由 App 下载并校验固定的 SenseVoice 2024-07-17 INT8 模型。
2. 点击“选择已有模型目录”，选择同时包含以下文件的目录：

```text
model.int8.onnx
tokens.txt
```

Silero VAD 模型已经包含在 App 中，无需单独配置。

### 剪辑音视频

1. 上传音频或视频文件，或将文件拖入 App。
2. 等待 VAD 检测和语音识别完成。
3. 修正识别文字，删除不需要的字幕片段，或恢复已删除片段。字幕编辑不支持手动换行。
4. 点击导出主按钮并选择保存位置。视频默认导出剪辑后的视频和字幕，音频默认导出剪辑后的音频和字幕。
5. 点击导出按钮右侧的箭头可选择其他方式：视频支持“导出音频和字幕”或“仅导出音频”，音频支持“仅导出字幕”。
6. 媒体导出会拼接保留片段并重新计算 `.srt` 时间轴；仅导出字幕时保留原始媒体时间轴。过长字幕会自动格式化为两行。

支持的输入格式：

```text
MP4、MOV、WAV、MP3、M4A、AAC、FLAC
```

视频和默认音频导出保持输入文件的容器格式，并尽可能保持原始音视频编码、分辨率和帧率。从视频导出音频时，App 会根据原音轨编码选择 M4A、MP3、FLAC 或 WAV 容器。
