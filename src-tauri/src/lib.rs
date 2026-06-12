mod app_info;
pub mod domain;
pub mod services;
mod task_state;

use std::path::{Path, PathBuf};

use app_info::AppInfo;
use domain::transcript::TranscriptSegment;
use services::exporter::{
    export_edited_media as run_export, ExportProgress, ExportRequest, ExportResult,
};
use services::model_manager::{
    configured_model_directory, inspect_model_directory, save_model_directory, ModelState,
    ModelStatus,
};
use services::speech::SpeechModelPaths;
use services::transcription::{
    transcribe_media as run_transcription, MediaKind, TranscriptionProgress, TranscriptionResult,
};
use task_state::ProcessingTaskState;
use tauri::{Emitter, Manager};

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
async fn transcribe_media(
    app: tauri::AppHandle,
    state: tauri::State<'_, ProcessingTaskState>,
    path: String,
) -> Result<TranscriptionResult, String> {
    let app_data = app_directories(&app)?;
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
        run_transcription(
            &source,
            model_paths,
            ffmpeg.as_deref(),
            &task_cancellation,
            |progress: TranscriptionProgress| {
                let _ = event_app.emit("transcription-progress", progress);
            },
        )
        .map_err(|error| error.to_string())
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
    segments: Vec<TranscriptSegment>,
) -> Result<ExportResult, String> {
    let ffmpeg =
        bundled_binary(&app, "ffmpeg").ok_or_else(|| "FFmpeg sidecar 尚未安装".to_owned())?;
    let ffprobe =
        bundled_binary(&app, "ffprobe").ok_or_else(|| "FFprobe sidecar 尚未安装".to_owned())?;
    let cancellation = state.begin().map_err(str::to_owned)?;
    let source = PathBuf::from(source_path);
    let destination = PathBuf::from(destination_path);
    let event_app = app.clone();
    let task_cancellation = cancellation.clone();

    let joined = tauri::async_runtime::spawn_blocking(move || {
        run_export(
            ExportRequest {
                ffmpeg: &ffmpeg,
                ffprobe: &ffprobe,
                source: &source,
                destination: &destination,
                media_kind,
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

fn bundled_binary(app: &tauri::AppHandle, binary: &str) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok();
    let executable_dir = std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf));
    let development_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
    let names = [
        binary.to_owned(),
        format!("{binary}-aarch64-apple-darwin"),
        format!("{binary}.exe"),
        format!("{binary}-x86_64-pc-windows-msvc.exe"),
    ];

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

fn bundled_ffmpeg(app: &tauri::AppHandle) -> Option<PathBuf> {
    bundled_binary(app, "ffmpeg")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ProcessingTaskState::default())
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_model_status,
            select_model_directory,
            transcribe_media,
            cancel_transcription,
            export_edited_media,
            cancel_export
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ARollCut");
}
