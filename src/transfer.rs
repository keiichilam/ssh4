//! Clipboard image and file transfer over SCP, with guaranteed temp cleanup.

use crate::ssh_client::{connect_session, ConnParams};
use std::path::{Path, PathBuf};

/// Deletes a temp path (file or directory) on drop, so cleanup runs on both
/// success and failure paths.
pub struct TempCleanup {
    path: PathBuf,
    armed: bool,
}

impl TempCleanup {
    pub fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }
    /// Keep the file (e.g. when handing ownership elsewhere).
    #[allow(dead_code)]
    pub fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if self.path.is_dir() {
            std::fs::remove_dir_all(&self.path).ok();
        } else {
            std::fs::remove_file(&self.path).ok();
        }
    }
}

/// Remote filename for a clipboard image: `clip_YYYYMMDD_HHMMSS.png`.
pub fn clip_filename(now: chrono::DateTime<chrono::Local>) -> String {
    format!("clip_{}.png", now.format("%Y%m%d_%H%M%S"))
}

/// Join a remote POSIX directory and filename.
pub fn remote_join(dir: &str, name: &str) -> String {
    let dir = dir.trim_end_matches('/');
    if dir.is_empty() {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

/// Capture the clipboard image to a temp PNG. Returns the temp path.
pub fn clipboard_image_to_temp_png() -> Result<PathBuf, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| format!("Clipboard error: {e}"))?;
    let img = clipboard
        .get_image()
        .map_err(|_| "No image in clipboard".to_string())?;
    let buffer: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned())
            .ok_or_else(|| "Could not decode clipboard image".to_string())?;
    let path = std::env::temp_dir().join(clip_filename(chrono::Local::now()));
    buffer
        .save(&path)
        .map_err(|e| format!("Could not save temp PNG: {e}"))?;
    Ok(path)
}

/// Upload a local path to the remote over a fresh SCP connection.
pub fn scp_upload(params: &ConnParams, local: &Path, remote: &str) -> Result<(), String> {
    let mut session = connect_session(params)?;
    let result = (|| {
        let scp = session
            .open_scp()
            .map_err(|e| crate::ssh_client::friendly_error(&e))?;
        scp.upload(local.as_os_str(), std::ffi::OsStr::new(remote))
            .map_err(|e| format!("Upload failed: {e}"))
    })();
    session.close();
    result
}

/// Full clipboard image upload flow. Returns the remote path on success.
/// The local temp PNG is removed on success and failure.
pub fn upload_clipboard_image(params: &ConnParams, remote_dir: &str) -> Result<String, String> {
    let local = clipboard_image_to_temp_png()?;
    let _cleanup = TempCleanup::new(local.clone());
    let name = local
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Bad temp filename".to_string())?
        .to_string();
    let remote = remote_join(remote_dir, &name);
    scp_upload(params, &local, &remote)?;
    // Copy the remote path to the local clipboard (best effort).
    if let Ok(mut cb) = arboard::Clipboard::new() {
        cb.set_text(remote.clone()).ok();
    }
    Ok(remote)
}

/// Stage dropped files/folders into a temp directory named `folder_name`,
/// returning the staging path. Caller wraps it in [`TempCleanup`].
pub fn stage_dropped_files(paths: &[PathBuf], folder_name: &str) -> Result<PathBuf, String> {
    let stage = std::env::temp_dir().join(format!(
        "ssh4_drop_{}_{}",
        std::process::id(),
        chrono::Local::now().format("%H%M%S")
    ));
    let target = stage.join(folder_name);
    std::fs::create_dir_all(&target).map_err(|e| format!("Could not stage files: {e}"))?;
    for src in paths {
        let Some(name) = src.file_name() else {
            continue;
        };
        let dst = target.join(name);
        if src.is_dir() {
            copy_dir(src, &dst).map_err(|e| format!("Copy failed: {e}"))?;
        } else {
            std::fs::copy(src, &dst).map_err(|e| format!("Copy failed: {e}"))?;
        }
    }
    Ok(stage)
}

pub fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Upload staged dropped files. Returns the remote folder path. The staging
/// directory is removed on success and failure.
pub fn upload_dropped_files(
    params: &ConnParams,
    paths: &[PathBuf],
    folder_name: &str,
    remote_dir: &str,
) -> Result<String, String> {
    let stage = stage_dropped_files(paths, folder_name)?;
    let _cleanup = TempCleanup::new(stage.clone());
    let local = stage.join(folder_name);
    let remote = remote_join(remote_dir, folder_name);
    scp_upload(params, &local, remote_dir)?;
    if let Ok(mut cb) = arboard::Clipboard::new() {
        cb.set_text(remote.clone()).ok();
    }
    Ok(remote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_filename_format() {
        use chrono::TimeZone;
        let t = chrono::Local
            .with_ymd_and_hms(2026, 6, 9, 14, 30, 5)
            .unwrap();
        assert_eq!(clip_filename(t), "clip_20260609_143005.png");
    }

    #[test]
    fn remote_join_rules() {
        assert_eq!(remote_join("/tmp", "a.png"), "/tmp/a.png");
        assert_eq!(remote_join("/tmp/", "a.png"), "/tmp/a.png");
        assert_eq!(remote_join("/", "a.png"), "/a.png");
        assert_eq!(remote_join("", "a.png"), "/a.png");
    }

    #[test]
    fn temp_cleanup_removes_file() {
        let path = std::env::temp_dir().join(format!("ssh4_clean_{}.tmp", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        {
            let _c = TempCleanup::new(path.clone());
        }
        assert!(!path.exists());
    }

    #[test]
    fn temp_cleanup_removes_dir() {
        let dir = std::env::temp_dir().join(format!("ssh4_cleand_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/f.txt"), "x").unwrap();
        {
            let _c = TempCleanup::new(dir.clone());
        }
        assert!(!dir.exists());
    }

    #[test]
    fn temp_cleanup_disarm_keeps_file() {
        let path = std::env::temp_dir().join(format!("ssh4_keep_{}.tmp", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        {
            let mut c = TempCleanup::new(path.clone());
            c.disarm();
        }
        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn staging_copies_files() {
        let src = std::env::temp_dir().join(format!("ssh4_src_{}.txt", std::process::id()));
        std::fs::write(&src, "data").unwrap();
        let stage = stage_dropped_files(&[src.clone()], "upload").unwrap();
        let _c = TempCleanup::new(stage.clone());
        let staged = stage.join("upload").join(src.file_name().unwrap());
        assert_eq!(std::fs::read_to_string(staged).unwrap(), "data");
        std::fs::remove_file(&src).ok();
    }
}
