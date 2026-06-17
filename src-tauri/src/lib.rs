mod app_info;
pub mod domain;
pub mod services;
mod task_state;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use app_info::AppInfo;
use domain::transcript::TranscriptSegment;
use services::exporter::{
    audio_export_extension, export_edited_media as run_export, ExportMode, ExportProgress,
    ExportRequest, ExportResult,
};
use services::model_manager::{
    configured_model_directory, configured_vad_settings, default_model_directory,
    download_model as run_model_download, inspect_model_directory,
    reset_vad_settings as persist_default_vad_settings, save_model_directory,
    save_vad_settings as persist_vad_settings, ModelDownloadProgress, ModelState, ModelStatus,
    VadSettings,
};
use services::speech::SpeechModelPaths;
use services::transcription::{
    transcribe_media as run_transcription, MediaKind, TranscriptionProgress, TranscriptionResult,
};
use task_state::ProcessingTaskState;
use tauri::{Emitter, Manager, RunEvent};

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo::current()
}

#[tauri::command]
fn get_model_status(app: tauri::AppHandle) -> Result<ModelStatus, String> {
    let (app_data_dir, app_config_dir) = app_directories(&app)?;
    let model_directory = configured_model_directory(&app_data_dir, &app_config_dir)
        .map_err(|error| format!("无法读取模型设置：{error}"))?;
    inspect_model_directory(&model_directory)
        .map_err(|error| format!("无法检查 SenseVoice 模型：{error}"))
}

#[tauri::command]
fn select_model_directory(app: tauri::AppHandle, directory: String) -> Result<ModelStatus, String> {
    let directory = std::path::PathBuf::from(directory);
    let status = inspect_model_directory(&directory)
        .map_err(|error| format!("无法检查 SenseVoice 模型：{error}"))?;
    if status.state != ModelState::Ready {
        return Ok(status);
    }

    let (_, app_config_dir) = app_directories(&app)?;
    save_model_directory(&app_config_dir, &directory)
        .map_err(|error| format!("无法保存模型设置：{error}"))?;
    Ok(status)
}

#[tauri::command]
fn get_vad_settings(app: tauri::AppHandle) -> Result<VadSettings, String> {
    let (_, app_config_dir) = app_directories(&app)?;
    configured_vad_settings(&app_config_dir).map_err(|error| format!("无法读取 VAD 设置：{error}"))
}

#[tauri::command]
fn save_vad_settings(app: tauri::AppHandle, settings: VadSettings) -> Result<VadSettings, String> {
    let (_, app_config_dir) = app_directories(&app)?;
    persist_vad_settings(&app_config_dir, settings)
        .map_err(|error| format!("无法保存 VAD 设置：{error}"))
}

#[tauri::command]
fn reset_vad_settings(app: tauri::AppHandle) -> Result<VadSettings, String> {
    let (_, app_config_dir) = app_directories(&app)?;
    persist_default_vad_settings(&app_config_dir)
        .map_err(|error| format!("无法恢复默认 VAD 设置：{error}"))
}

#[tauri::command]
async fn download_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, ProcessingTaskState>,
) -> Result<ModelStatus, String> {
    let (app_data_dir, app_config_dir) = app_directories(&app)?;
    let destination = default_model_directory(&app_data_dir);
    let cancellation = state.begin().map_err(str::to_owned)?;
    let task_cancellation = cancellation.clone();
    let event_app = app.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let status = run_model_download(
            &destination,
            &task_cancellation,
            |progress: ModelDownloadProgress| {
                let _ = event_app.emit("model-download-progress", progress);
            },
        )
        .map_err(|error| error.to_string())?;
        save_model_directory(&app_config_dir, &destination)
            .map_err(|error| format!("模型已下载，但无法保存模型目录设置：{error}"))?;
        Ok(status)
    })
    .await;

    state.finish(&cancellation);
    joined.map_err(|error| format!("模型下载任务异常终止：{error}"))?
}

#[tauri::command]
fn cancel_model_download(state: tauri::State<'_, ProcessingTaskState>) -> bool {
    state.cancel()
}

#[tauri::command]
async fn transcribe_media(
    app: tauri::AppHandle,
    state: tauri::State<'_, ProcessingTaskState>,
    path: String,
) -> Result<TranscriptionResult, String> {
    let app_data = app_directories(&app)?;
    let preview_audio = preview_audio_path(&app)?;
    let vad_model = bundled_vad_model(&app)?;
    let ffmpeg = bundled_ffmpeg(&app);
    let cancellation = state.begin().map_err(str::to_owned)?;
    let source = PathBuf::from(path);
    let event_app = app.clone();
    let task_cancellation = cancellation.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        let model_directory = configured_model_directory(&app_data.0, &app_data.1)
            .map_err(|error| format!("无法读取模型设置：{error}"))?;
        let status = inspect_model_directory(&model_directory)
            .map_err(|error| format!("无法检查 SenseVoice 模型：{error}"))?;
        if status.state != ModelState::Ready {
            return Err(format!(
                "SenseVoice 模型不可用：{}",
                status.issues.join("；")
            ));
        }

        let model_paths = SpeechModelPaths {
            vad_model,
            sense_voice_model: model_directory.join("model.int8.onnx"),
            tokens: model_directory.join("tokens.txt"),
        };
        let vad_settings = configured_vad_settings(&app_data.1)
            .map_err(|error| format!("无法读取 VAD 设置：{error}"))?;
        run_transcription(
            &source,
            model_paths,
            vad_settings,
            &preview_audio,
            ffmpeg.as_deref(),
            &task_cancellation,
            |progress: TranscriptionProgress| {
                let _ = event_app.emit("transcription-progress", progress);
            },
        )
        .map_err(|error| {
            let _ = fs::remove_file(&preview_audio);
            error.to_string()
        })
    })
    .await;

    state.finish(&cancellation);
    let task = joined.map_err(|error| format!("识别任务异常终止：{error}"))?;
    task
}

#[tauri::command]
fn cancel_transcription(state: tauri::State<'_, ProcessingTaskState>) -> bool {
    state.cancel()
}

#[tauri::command]
async fn export_edited_media(
    app: tauri::AppHandle,
    state: tauri::State<'_, ProcessingTaskState>,
    source_path: String,
    destination_path: String,
    media_kind: MediaKind,
    mode: ExportMode,
    segments: Vec<TranscriptSegment>,
) -> Result<ExportResult, String> {
    let ffmpeg = if mode.requires_media_tools() {
        Some(bundled_binary(&app, "ffmpeg").ok_or_else(|| "FFmpeg sidecar 尚未安装".to_owned())?)
    } else {
        None
    };
    let ffprobe = if mode.requires_media_tools() {
        Some(bundled_binary(&app, "ffprobe").ok_or_else(|| "FFprobe sidecar 尚未安装".to_owned())?)
    } else {
        None
    };
    let cancellation = state.begin().map_err(str::to_owned)?;
    let source = PathBuf::from(source_path);
    let destination = PathBuf::from(destination_path);
    let event_app = app.clone();
    let task_cancellation = cancellation.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        run_export(
            ExportRequest {
                ffmpeg: ffmpeg.as_deref(),
                ffprobe: ffprobe.as_deref(),
                source: &source,
                destination: &destination,
                media_kind,
                mode,
                segments: &segments,
            },
            &task_cancellation,
            |progress: ExportProgress| {
                let _ = event_app.emit("export-progress", progress);
            },
        )
        .map_err(|error| error.to_string())
    })
    .await;

    state.finish(&cancellation);
    joined.map_err(|error| format!("导出任务异常终止：{error}"))?
}

#[tauri::command]
fn get_audio_export_extension(
    app: tauri::AppHandle,
    source_path: String,
) -> Result<String, String> {
    let ffprobe =
        bundled_binary(&app, "ffprobe").ok_or_else(|| "FFprobe sidecar 尚未安装".to_owned())?;
    audio_export_extension(&ffprobe, Path::new(&source_path)).map_err(|error| error.to_string())
}

#[tauri::command]
fn cancel_export(state: tauri::State<'_, ProcessingTaskState>) -> bool {
    state.cancel()
}

fn app_directories(
    app: &tauri::AppHandle,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("无法获取 App Data 目录：{error}"))?;
    let app_config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法获取 App Config 目录：{error}"))?;
    Ok((app_data_dir, app_config_dir))
}

fn bundled_vad_model(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("无法获取 App 资源目录：{error}"))?;
    let candidates = [
        resource_dir.join("resources/silero_vad.onnx"),
        resource_dir.join("silero_vad.onnx"),
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("silero_vad.onnx"),
    ];

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "App 内置的 Silero VAD 模型不存在".to_owned())
}

fn preview_audio_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = preview_audio_directory(app)?;
    fs::create_dir_all(&directory).map_err(|error| format!("无法创建预览音频目录：{error}"))?;
    cleanup_preview_audio_directory(&directory);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    Ok(directory.join(format!("preview-{timestamp}.wav")))
}

fn preview_audio_directory(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(|error| format!("无法获取 App Cache 目录：{error}"))?
        .join("preview-audio"))
}

fn cleanup_preview_audio_cache(app: &tauri::AppHandle) {
    if let Ok(directory) = preview_audio_directory(app) {
        cleanup_preview_audio_directory(&directory);
    }
}

fn cleanup_preview_audio_directory(directory: &Path) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("wav"))
        {
            let _ = fs::remove_file(path);
        }
    }
}

fn bundled_binary(app: &tauri::AppHandle, binary: &str) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let development_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let names = bundled_binary_names(binary, std::env::consts::OS, std::env::consts::ARCH);

    let directories = resource_dir
        .into_iter()
        .chain(executable_dir)
        .chain(std::iter::once(development_dir))
        .collect::<Vec<_>>();
    for directory in directories {
        for name in &names {
            for candidate in [directory.join(name), directory.join("binaries").join(name)] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn bundled_binary_names(binary: &str, os: &str, architecture: &str) -> Vec<String> {
    let generic_name = if os == "windows" {
        format!("{binary}.exe")
    } else {
        binary.to_owned()
    };
    let target_name = match (os, architecture) {
        ("macos", "aarch64") => Some(format!("{binary}-aarch64-apple-darwin")),
        ("windows", "x86_64") => Some(format!("{binary}-x86_64-pc-windows-msvc.exe")),
        _ => None,
    };

    std::iter::once(generic_name).chain(target_name).collect()
}

fn bundled_ffmpeg(app: &tauri::AppHandle) -> Option<PathBuf> {
    bundled_binary(app, "ffmpeg")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingTaskState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_model_status,
            select_model_directory,
            get_vad_settings,
            save_vad_settings,
            reset_vad_settings,
            download_model,
            cancel_model_download,
            transcribe_media,
            cancel_transcription,
            get_audio_export_extension,
            export_edited_media,
            cancel_export
        ])
        .build(tauri::generate_context!())
        .expect("failed to build ARollCut");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::Exit) {
            cleanup_preview_audio_cache(app_handle);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::{bundled_binary_names, cleanup_preview_audio_directory};

    #[test]
    fn resolves_macos_apple_silicon_sidecar_names() {
        assert_eq!(
            bundled_binary_names("ffmpeg", "macos", "aarch64"),
            vec!["ffmpeg", "ffmpeg-aarch64-apple-darwin"]
        );
    }

    #[test]
    fn resolves_windows_x64_sidecar_names() {
        assert_eq!(
            bundled_binary_names("ffprobe", "windows", "x86_64"),
            vec!["ffprobe.exe", "ffprobe-x86_64-pc-windows-msvc.exe"]
        );
    }

    #[test]
    fn preview_audio_cleanup_removes_only_cached_wav_files() {
        let directory = tempdir().expect("preview directory");
        let cached_wave = directory.path().join("preview.wav");
        let nested_wave = directory.path().join("nested").join("preview.wav");
        let note = directory.path().join("note.txt");
        fs::write(&cached_wave, b"wave").expect("write cached wave");
        fs::create_dir_all(nested_wave.parent().expect("nested parent")).expect("nested dir");
        fs::write(&nested_wave, b"nested wave").expect("write nested wave");
        fs::write(&note, b"note").expect("write note");

        cleanup_preview_audio_directory(directory.path());

        assert!(!cached_wave.exists());
        assert!(nested_wave.exists());
        assert!(note.exists());
    }
}
