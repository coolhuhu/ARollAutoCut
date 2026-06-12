use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use bzip2::read::BzDecoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MODEL_DIRECTORY_NAME: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17";
pub const MODEL_ARCHIVE_NAME: &str = "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2";
pub const MODEL_DOWNLOAD_URL: &str = concat!(
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/",
    "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2"
);

const MODEL_FILES: [ModelFileSpec; 2] = [
    ModelFileSpec {
        name: "model.int8.onnx",
        size: 239_233_841,
        sha256: "c71f0ce00bec95b07744e116345e33d8cbbe08cef896382cf907bf4b51a2cd51",
    },
    ModelFileSpec {
        name: "tokens.txt",
        size: 315_894,
        sha256: "f449eb28dc567533d7fa59be34e2abca8784f771850c78a47fb731a31429a1dc",
    },
];

#[derive(Debug, Clone, Copy)]
struct ModelFileSpec {
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelState {
    Missing,
    Invalid,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub state: ModelState,
    pub directory: PathBuf,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub stage: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub percent: u8,
    pub message: String,
}

#[derive(Debug)]
pub enum ModelDownloadError {
    Request(reqwest::Error),
    Io(io::Error),
    InvalidArchive(String),
    Validation(Vec<String>),
    Cancelled,
}

impl std::fmt::Display for ModelDownloadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Request(error) => write!(formatter, "模型下载失败：{error}"),
            Self::Io(error) => write!(formatter, "模型文件处理失败：{error}"),
            Self::InvalidArchive(message) => write!(formatter, "模型压缩包无效：{message}"),
            Self::Validation(issues) => write!(formatter, "模型校验失败：{}", issues.join("；")),
            Self::Cancelled => write!(formatter, "模型下载已取消"),
        }
    }
}

impl std::error::Error for ModelDownloadError {}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelSettings {
    model_directory: Option<PathBuf>,
}

pub fn default_model_directory(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join(MODEL_DIRECTORY_NAME)
}

pub fn configured_model_directory(
    app_data_dir: &Path,
    app_config_dir: &Path,
) -> io::Result<PathBuf> {
    let settings_path = settings_path(app_config_dir);
    if !settings_path.is_file() {
        return Ok(default_model_directory(app_data_dir));
    }

    let settings: ModelSettings = serde_json::from_slice(&fs::read(settings_path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(settings
        .model_directory
        .unwrap_or_else(|| default_model_directory(app_data_dir)))
}

pub fn save_model_directory(app_config_dir: &Path, directory: &Path) -> io::Result<()> {
    fs::create_dir_all(app_config_dir)?;
    let settings_path = settings_path(app_config_dir);
    let temporary_path = settings_path.with_extension("json.tmp");
    let settings = ModelSettings {
        model_directory: Some(directory.to_path_buf()),
    };
    let contents = serde_json::to_vec_pretty(&settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    fs::write(&temporary_path, contents)?;
    fs::rename(temporary_path, settings_path)?;
    Ok(())
}

pub fn inspect_model_directory(directory: &Path) -> io::Result<ModelStatus> {
    inspect_with_manifest(directory, &MODEL_FILES)
}

pub fn download_model(
    destination: &Path,
    cancelled: &AtomicBool,
    mut on_progress: impl FnMut(ModelDownloadProgress),
) -> Result<ModelStatus, ModelDownloadError> {
    let parent = destination
        .parent()
        .ok_or_else(|| ModelDownloadError::InvalidArchive("模型目录缺少父目录".to_owned()))?;
    fs::create_dir_all(parent).map_err(ModelDownloadError::Io)?;
    let archive_path = parent.join(format!(".{MODEL_ARCHIVE_NAME}.download"));
    let staging_path = parent.join(format!(".{MODEL_DIRECTORY_NAME}.installing"));
    remove_path_if_exists(&archive_path).map_err(ModelDownloadError::Io)?;
    remove_directory_if_exists(&staging_path).map_err(ModelDownloadError::Io)?;

    let result = (|| {
        download_archive(&archive_path, cancelled, &mut on_progress)?;
        check_download_cancelled(cancelled)?;
        on_progress(download_progress(
            "extracting",
            0,
            None,
            92,
            "正在解压 INT8 模型文件",
        ));
        fs::create_dir_all(&staging_path).map_err(ModelDownloadError::Io)?;
        extract_required_files(&archive_path, &staging_path)?;

        check_download_cancelled(cancelled)?;
        on_progress(download_progress(
            "verifying",
            0,
            None,
            97,
            "正在校验文件大小和 SHA-256",
        ));
        let status = inspect_model_directory(&staging_path).map_err(ModelDownloadError::Io)?;
        if status.state != ModelState::Ready {
            return Err(ModelDownloadError::Validation(status.issues));
        }

        remove_directory_if_exists(destination).map_err(ModelDownloadError::Io)?;
        fs::rename(&staging_path, destination).map_err(ModelDownloadError::Io)?;
        on_progress(download_progress(
            "complete",
            0,
            None,
            100,
            "SenseVoice 模型安装完成",
        ));
        inspect_model_directory(destination).map_err(ModelDownloadError::Io)
    })();

    let _ = remove_path_if_exists(&archive_path);
    if result.is_err() {
        let _ = remove_directory_if_exists(&staging_path);
    }
    result
}

fn download_archive(
    archive_path: &Path,
    cancelled: &AtomicBool,
    on_progress: &mut dyn FnMut(ModelDownloadProgress),
) -> Result<(), ModelDownloadError> {
    if !MODEL_DOWNLOAD_URL.ends_with(MODEL_ARCHIVE_NAME) {
        return Err(ModelDownloadError::InvalidArchive(
            "官方下载地址中的文件名不匹配".to_owned(),
        ));
    }

    on_progress(download_progress(
        "downloading",
        0,
        None,
        0,
        "正在连接 sherpa-onnx 官方下载地址",
    ));
    let client = reqwest::blocking::Client::builder()
        .user_agent("ARollCut/0.1")
        .build()
        .map_err(ModelDownloadError::Request)?;
    let mut response = client
        .get(MODEL_DOWNLOAD_URL)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(ModelDownloadError::Request)?;
    let total_bytes = response.content_length();
    let mut archive = File::create(archive_path).map_err(ModelDownloadError::Io)?;
    let mut downloaded_bytes = 0_u64;
    let mut buffer = [0_u8; 128 * 1024];

    loop {
        check_download_cancelled(cancelled)?;
        let read = response.read(&mut buffer).map_err(ModelDownloadError::Io)?;
        if read == 0 {
            break;
        }
        archive
            .write_all(&buffer[..read])
            .map_err(ModelDownloadError::Io)?;
        downloaded_bytes += read as u64;
        let percent = total_bytes
            .filter(|total| *total > 0)
            .map(|total| ((downloaded_bytes.saturating_mul(90) / total).min(90)) as u8)
            .unwrap_or(0);
        on_progress(download_progress(
            "downloading",
            downloaded_bytes,
            total_bytes,
            percent,
            "正在下载 SenseVoice 模型",
        ));
    }
    archive.flush().map_err(ModelDownloadError::Io)?;

    if let Some(expected) = total_bytes {
        if downloaded_bytes != expected {
            return Err(ModelDownloadError::InvalidArchive(format!(
                "下载大小不完整：预期 {expected} 字节，实际 {downloaded_bytes} 字节"
            )));
        }
    }
    Ok(())
}

fn extract_required_files(
    archive_path: &Path,
    destination: &Path,
) -> Result<(), ModelDownloadError> {
    let archive_file = File::open(archive_path).map_err(ModelDownloadError::Io)?;
    let decoder = BzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let mut extracted = Vec::new();

    for entry in archive.entries().map_err(ModelDownloadError::Io)? {
        let mut entry = entry.map_err(ModelDownloadError::Io)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(ModelDownloadError::Io)?;
        let Some(file_name) = path
            .file_name()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !MODEL_FILES.iter().any(|spec| spec.name == file_name) {
            continue;
        }

        let output = destination.join(&file_name);
        entry.unpack(&output).map_err(ModelDownloadError::Io)?;
        extracted.push(file_name);
    }

    for spec in MODEL_FILES {
        if !extracted.iter().any(|name| name == spec.name) {
            return Err(ModelDownloadError::InvalidArchive(format!(
                "压缩包中缺少 {}",
                spec.name
            )));
        }
    }
    Ok(())
}

fn check_download_cancelled(cancelled: &AtomicBool) -> Result<(), ModelDownloadError> {
    if cancelled.load(Ordering::Relaxed) {
        Err(ModelDownloadError::Cancelled)
    } else {
        Ok(())
    }
}

fn download_progress(
    stage: &'static str,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    percent: u8,
    message: &str,
) -> ModelDownloadProgress {
    ModelDownloadProgress {
        stage,
        downloaded_bytes,
        total_bytes,
        percent,
        message: message.to_owned(),
    }
}

fn remove_path_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_directory_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn settings_path(app_config_dir: &Path) -> PathBuf {
    app_config_dir.join("settings.json")
}

fn inspect_with_manifest(directory: &Path, manifest: &[ModelFileSpec]) -> io::Result<ModelStatus> {
    let mut missing = Vec::new();
    let mut invalid = Vec::new();

    for spec in manifest {
        let path = directory.join(spec.name);
        if !path.is_file() {
            missing.push(format!("缺少文件：{}", spec.name));
            continue;
        }

        let actual_size = path.metadata()?.len();
        if actual_size != spec.size {
            invalid.push(format!(
                "{} 大小不正确：预期 {} 字节，实际 {} 字节",
                spec.name, spec.size, actual_size
            ));
            continue;
        }

        let actual_hash = sha256_file(&path)?;
        if actual_hash != spec.sha256 {
            invalid.push(format!("{} 的 SHA-256 校验失败", spec.name));
        }
    }

    let (state, issues) = if !missing.is_empty() {
        missing.extend(invalid);
        (ModelState::Missing, missing)
    } else if !invalid.is_empty() {
        (ModelState::Invalid, invalid)
    } else {
        (ModelState::Ready, Vec::new())
    };

    Ok(ModelStatus {
        state,
        directory: directory.to_path_buf(),
        issues,
    })
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use bzip2::write::BzEncoder;
    use bzip2::Compression;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    use super::{
        configured_model_directory, default_model_directory, extract_required_files,
        inspect_with_manifest, save_model_directory, ModelFileSpec, ModelState, MODEL_ARCHIVE_NAME,
        MODEL_DIRECTORY_NAME, MODEL_DOWNLOAD_URL,
    };

    #[test]
    fn builds_the_model_path_inside_app_data() {
        let directory = default_model_directory(Path::new("/app-data"));

        assert_eq!(
            directory,
            Path::new("/app-data")
                .join("models")
                .join(MODEL_DIRECTORY_NAME)
        );
    }

    #[test]
    fn reports_missing_files() {
        let directory = tempdir().expect("temp directory");
        let manifest = [spec("model.onnx", b"model")];

        let status = inspect_with_manifest(directory.path(), &manifest).expect("status");

        assert_eq!(status.state, ModelState::Missing);
        assert_eq!(status.issues, vec!["缺少文件：model.onnx"]);
    }

    #[test]
    fn reports_invalid_size_before_hashing() {
        let directory = tempdir().expect("temp directory");
        fs::write(directory.path().join("model.onnx"), b"bad").expect("write model");
        let manifest = [spec("model.onnx", b"expected")];

        let status = inspect_with_manifest(directory.path(), &manifest).expect("status");

        assert_eq!(status.state, ModelState::Invalid);
        assert!(status.issues[0].contains("大小不正确"));
    }

    #[test]
    fn accepts_files_matching_size_and_sha256() {
        let directory = tempdir().expect("temp directory");
        fs::write(directory.path().join("model.onnx"), b"model").expect("write model");
        fs::write(directory.path().join("tokens.txt"), b"tokens").expect("write tokens");
        let manifest = [spec("model.onnx", b"model"), spec("tokens.txt", b"tokens")];

        let status = inspect_with_manifest(directory.path(), &manifest).expect("status");

        assert_eq!(status.state, ModelState::Ready);
        assert!(status.issues.is_empty());
    }

    #[test]
    fn uses_default_directory_when_no_setting_exists() {
        let directory = tempdir().expect("temp directory");
        let app_data = directory.path().join("data");
        let app_config = directory.path().join("config");

        assert_eq!(
            configured_model_directory(&app_data, &app_config).expect("configured directory"),
            default_model_directory(&app_data)
        );
    }

    #[test]
    fn persists_a_selected_model_directory() {
        let directory = tempdir().expect("temp directory");
        let app_data = directory.path().join("data");
        let app_config = directory.path().join("config");
        let selected = directory.path().join("selected-model");

        save_model_directory(&app_config, &selected).expect("save model directory");

        assert_eq!(
            configured_model_directory(&app_data, &app_config).expect("configured directory"),
            selected
        );
    }

    #[test]
    fn official_download_url_uses_the_real_release_asset_name() {
        assert!(MODEL_DOWNLOAD_URL.ends_with(MODEL_ARCHIVE_NAME));
        assert!(!MODEL_ARCHIVE_NAME.contains("-int8-"));
    }

    #[test]
    fn extracts_only_the_required_int8_model_files() {
        let directory = tempdir().expect("temp directory");
        let archive_path = directory.path().join("model.tar.bz2");
        let destination = directory.path().join("model");
        fs::create_dir_all(&destination).expect("destination");
        write_test_archive(
            &archive_path,
            &[
                ("release/model.int8.onnx", b"int8 model"),
                ("release/model.onnx", b"full model"),
                ("release/tokens.txt", b"tokens"),
                ("release/README.md", b"readme"),
            ],
        );

        extract_required_files(&archive_path, &destination).expect("extract model");

        assert_eq!(
            fs::read(destination.join("model.int8.onnx")).expect("int8 model"),
            b"int8 model"
        );
        assert_eq!(
            fs::read(destination.join("tokens.txt")).expect("tokens"),
            b"tokens"
        );
        assert!(!destination.join("model.onnx").exists());
        assert!(!destination.join("README.md").exists());
    }

    #[test]
    fn rejects_an_archive_missing_a_required_file() {
        let directory = tempdir().expect("temp directory");
        let archive_path = directory.path().join("model.tar.bz2");
        let destination = directory.path().join("model");
        fs::create_dir_all(&destination).expect("destination");
        write_test_archive(&archive_path, &[("release/model.int8.onnx", b"int8 model")]);

        let error =
            extract_required_files(&archive_path, &destination).expect_err("missing tokens");

        assert!(error.to_string().contains("tokens.txt"));
    }

    fn spec(name: &'static str, contents: &[u8]) -> ModelFileSpec {
        ModelFileSpec {
            name,
            size: contents.len() as u64,
            sha256: Box::leak(format!("{:x}", Sha256::digest(contents)).into_boxed_str()),
        }
    }

    fn write_test_archive(path: &Path, files: &[(&str, &[u8])]) {
        let encoder = BzEncoder::new(
            File::create(path).expect("create archive"),
            Compression::best(),
        );
        let mut archive = tar::Builder::new(encoder);
        for (name, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            archive
                .append_data(&mut header, name, *contents)
                .expect("append archive entry");
        }
        let encoder = archive.into_inner().expect("finish tar");
        encoder.finish().expect("finish bzip2");
    }

    use std::path::Path;
}
