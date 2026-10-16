use std::{path::PathBuf, sync::OnceLock};

static BUNDLE_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();

#[cfg(feature = "desktop")]
pub fn set_bundle_directory(directory: PathBuf) {
    let _ = BUNDLE_DIRECTORY.set(directory);
}

/// Installed Tauri resources first, then portable and development overrides.
pub fn directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(directory) = BUNDLE_DIRECTORY.get() {
        directories.push(directory.clone());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(directory) = exe.parent() {
            #[cfg(target_os = "macos")]
            if let Some(contents) = directory.parent() {
                directories.push(contents.join("Resources"));
            }
            #[cfg(target_os = "linux")]
            if let Some(prefix) = directory.parent() {
                directories.push(prefix.join("lib/maploom"));
            }
            directories.push(directory.to_path_buf());
        }
    }
    if let Ok(directory) = std::env::current_dir() {
        directories.push(directory);
    }
    directories
}
