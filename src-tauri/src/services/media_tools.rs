use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MediaToolPaths {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl MediaToolPaths {
    pub fn validate(&self) -> Result<(), MediaToolError> {
        for (name, path) in [("ffmpeg", &self.ffmpeg), ("ffprobe", &self.ffprobe)] {
            if !path.is_file() {
                return Err(MediaToolError::MissingExecutable {
                    name,
                    path: path.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn probe_versions(&self) -> Result<MediaToolVersions, MediaToolError> {
        self.validate()?;
        Ok(MediaToolVersions {
            ffmpeg: first_version_line(run_version(&self.ffmpeg)?),
            ffprobe: first_version_line(run_version(&self.ffprobe)?),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaToolVersions {
    pub ffmpeg: String,
    pub ffprobe: String,
}

#[derive(Debug)]
pub enum MediaToolError {
    MissingExecutable {
        name: &'static str,
        path: PathBuf,
    },
    Spawn {
        path: PathBuf,
        source: std::io::Error,
    },
    Failed {
        path: PathBuf,
        stderr: String,
    },
    Cancelled,
}

impl fmt::Display for MediaToolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingExecutable { name, path } => {
                write!(formatter, "{name} sidecar 不存在：{}", path.display())
            }
            Self::Spawn { path, source } => {
                write!(formatter, "无法启动 {}：{source}", path.display())
            }
            Self::Failed { path, stderr } => {
                write!(formatter, "{} 执行失败：{stderr}", path.display())
            }
            Self::Cancelled => write!(formatter, "媒体处理已取消"),
        }
    }
}

impl Error for MediaToolError {}

pub struct RunningMediaCommand {
    child: Child,
}

impl RunningMediaCommand {
    pub fn spawn(program: &Path, arguments: &[&str]) -> Result<Self, MediaToolError> {
        let child = Command::new(program)
            .args(arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| MediaToolError::Spawn {
                path: program.into(),
                source,
            })?;
        Ok(Self { child })
    }

    pub fn cancel(&mut self) -> std::io::Result<()> {
        self.child.kill()
    }

    pub fn wait(self) -> std::io::Result<Output> {
        self.child.wait_with_output()
    }
}

pub fn extract_speech_wave(
    ffmpeg: &Path,
    source: &Path,
    destination: &Path,
    is_cancelled: &dyn Fn() -> bool,
) -> Result<(), MediaToolError> {
    if !ffmpeg.is_file() {
        return Err(MediaToolError::MissingExecutable {
            name: "ffmpeg",
            path: ffmpeg.into(),
        });
    }

    let mut child = Command::new(ffmpeg)
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-y")
        .arg("-i")
        .arg(source)
        .arg("-map")
        .arg("0:a:0")
        .arg("-vn")
        .arg("-ac")
        .arg("1")
        .arg("-ar")
        .arg("16000")
        .arg("-c:a")
        .arg("pcm_s16le")
        .arg(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| MediaToolError::Spawn {
            path: ffmpeg.into(),
            source,
        })?;

    loop {
        if is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(MediaToolError::Cancelled);
        }

        match child.try_wait().map_err(|source| MediaToolError::Spawn {
            path: ffmpeg.into(),
            source,
        })? {
            Some(status) => {
                let output = child
                    .wait_with_output()
                    .map_err(|source| MediaToolError::Spawn {
                        path: ffmpeg.into(),
                        source,
                    })?;
                if status.success() {
                    return Ok(());
                }
                return Err(MediaToolError::Failed {
                    path: ffmpeg.into(),
                    stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                });
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn run_version(path: &Path) -> Result<Output, MediaToolError> {
    let output = Command::new(path)
        .arg("-version")
        .output()
        .map_err(|source| MediaToolError::Spawn {
            path: path.into(),
            source,
        })?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(MediaToolError::Failed {
            path: path.into(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

fn first_version_line(output: Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use tempfile::tempdir;

    use super::{extract_speech_wave, MediaToolError, MediaToolPaths, RunningMediaCommand};

    #[test]
    fn reads_versions_from_bundled_style_executables() {
        let directory = tempdir().expect("temp directory");
        let ffmpeg = directory.path().join("ffmpeg");
        let ffprobe = directory.path().join("ffprobe");
        write_executable(&ffmpeg, "#!/bin/sh\necho 'ffmpeg version test'\n");
        write_executable(&ffprobe, "#!/bin/sh\necho 'ffprobe version test'\n");

        let versions = MediaToolPaths { ffmpeg, ffprobe }
            .probe_versions()
            .expect("versions");

        assert_eq!(versions.ffmpeg, "ffmpeg version test");
        assert_eq!(versions.ffprobe, "ffprobe version test");
    }

    #[test]
    fn a_running_media_process_can_be_cancelled() {
        let mut command =
            RunningMediaCommand::spawn(Path::new("/bin/sleep"), &["5"]).expect("spawn sleep");

        command.cancel().expect("cancel process");
        let output = command.wait().expect("wait for cancelled process");

        assert!(!output.status.success());
    }

    #[test]
    fn extracts_a_speech_wave_with_expected_ffmpeg_arguments() {
        let directory = tempdir().expect("temp directory");
        let ffmpeg = directory.path().join("ffmpeg");
        let source = directory.path().join("source audio.m4a");
        let destination = directory.path().join("speech.wav");
        fs::write(&source, b"source").expect("write source");
        write_executable(
            &ffmpeg,
            "#!/bin/sh\nfor last do :; done\nprintf 'wave' > \"$last\"\n",
        );

        extract_speech_wave(&ffmpeg, &source, &destination, &|| false).expect("extract wave");

        assert_eq!(fs::read(destination).expect("read wave"), b"wave");
    }

    #[test]
    fn cancels_ffmpeg_preprocessing() {
        let directory = tempdir().expect("temp directory");
        let ffmpeg = directory.path().join("ffmpeg");
        let source = directory.path().join("source.m4a");
        let destination = directory.path().join("speech.wav");
        fs::write(&source, b"source").expect("write source");
        write_executable(&ffmpeg, "#!/bin/sh\nsleep 5\n");

        let error = extract_speech_wave(&ffmpeg, &source, &destination, &|| true)
            .expect_err("preprocessing should be cancelled");

        assert!(matches!(error, MediaToolError::Cancelled));
    }

    fn write_executable(path: &Path, contents: &str) {
        fs::write(path, contents).expect("write executable");
        let mut permissions = fs::metadata(path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("set permissions");
    }
}
