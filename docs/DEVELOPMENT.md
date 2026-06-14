# ARollCut 开发说明

本文记录不属于安装和编译指南的开发约束、平台状态及后续验收事项。仓库中的自动化代理和贡献者还必须遵守根目录的 [AGENTS.md](../AGENTS.md)。

## 技术栈

- Tauri 2
- React 19
- TypeScript
- Vite
- Rust
- sherpa-onnx 1.13.2
- SenseVoice Small 2024-07-17 INT8
- Silero VAD
- FFmpeg 8.1.1

## 产品约束

- 第一版面向 macOS Apple Silicon，最低支持 macOS 12。
- Windows 目标平台为 x64。
- Silero VAD 随 App 分发。
- SenseVoice 模型不随 App 分发，只允许使用固定的 2024-07-17 INT8 模型。
- VAD 决定有效语音范围；长转写片段会依据 SenseVoice token 时间戳进一步拆分，
  媒体剪辑使用拆分后连续的字幕时间边界。
- 导出媒体不烧录字幕，同时生成重新计算时间轴的 SRT。
- 导出需要通过 FFprobe 继承源媒体的 codec、码率、Profile、像素格式、帧率和
  B 帧配置；FFmpeg 编码进度通过 `-progress` 持续上报。
- 不保存项目，不提供自动保存；关闭未完成编辑时需要用户确认。

## 模型管理

SenseVoice 模型支持从 sherpa-onnx 官方 Release 下载，或选择已有模型目录。可用目录必须包含：

```text
model.int8.onnx
tokens.txt
```

下载流程需要支持进度显示、取消、失败重试，以及文件名、大小和 SHA-256 校验。识别任务进行期间不得切换模型。

## FFmpeg sidecar

FFmpeg 和 FFprobe 必须作为 Tauri sidecar 分发，并固定使用 FFmpeg 8.1.1。新增或替换二进制时，需要同步记录：

- 下载来源或构建来源
- 构建参数
- SHA-256
- 第三方许可证

当前 macOS Apple Silicon sidecar 的 SHA-256：

```text
ffmpeg:  b7f8243043f73401acb928c2fac9d6f20f123853ff21b3bd2019fdc87de2835d
ffprobe: 29435b07c07c0f8ea53666c96a5db5dcbcc3053e77fd6125e5396501b6a9a4ad
```

## Windows x64 状态

Windows 使用 `src-tauri/tauri.windows.conf.json` 生成 NSIS 安装包。Tauri 配置和目标感知的 sidecar 查找规则已经准备完成，但尚未在 Windows 实机验收。

Windows 构建需要提供：

```text
src-tauri/binaries/ffmpeg-x86_64-pc-windows-msvc.exe
src-tauri/binaries/ffprobe-x86_64-pc-windows-msvc.exe
```

Windows FFmpeg 至少需要包含：

```text
libx264、libx265、prores_ks、aac、alac、flac、libmp3lame、常用 PCM 编码器
```

`sherpa-onnx 1.13.2` 对应的官方 Windows x64 静态运行库为：

```text
sherpa-onnx-v1.13.2-win-x64-static-MT-Release-lib.tar.bz2
```

## Windows 验收清单

1. 运行前端、Rust 单元测试和 release 构建。
2. 验证 SenseVoice 模型下载、取消、重试和校验。
3. 导入 WAV、MP3、MP4 和 MOV，完成 VAD 与转写。
4. 删除、恢复和修正字幕后导出媒体与 SRT。
5. 验证覆盖已有导出、取消导出、关闭 App 和重新上传确认。
6. 使用 ffprobe 对比输入输出的 codec、分辨率、帧率和音频采样率。
