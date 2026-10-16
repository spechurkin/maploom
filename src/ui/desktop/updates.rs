//! Only native commands can install updates, after rechecking the authoritative document.
use super::DesktopState;
use serde::Serialize;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tauri::{Manager, ipc::Channel};
use tauri_plugin_updater::{Update, UpdaterExt};

#[derive(Default)]
pub(super) struct UpdateState {
    pending: Mutex<Option<Update>>,
    pub(super) installing: AtomicBool,
}

#[derive(Serialize)]
pub(super) struct UpdateInfo {
    status: &'static str,
    version: Option<String>,
    notes: Option<String>,
    install_blocked: Option<&'static str>,
}

fn install_blocked() -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("Update development build");
    }
    #[cfg(target_os = "linux")]
    if std::env::var_os("APPIMAGE").is_none() && tauri::utils::platform::bundle_type().is_none() {
        return Some("Update AppImage only");
    }
    #[cfg(target_os = "macos")]
    if !std::env::current_exe().is_ok_and(|p| {
        p.ancestors()
            .any(|p| p.extension().is_some_and(|e| e == "app"))
    }) {
        return Some("Update app bundle only");
    }
    None
}

#[tauri::command]
pub(super) async fn update_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
) -> Result<UpdateInfo, String> {
    let result = app
        .updater_builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?
        .check()
        .await;
    let (status, update) = match result {
        Ok(Some(update)) => ("available", Some(update)),
        Ok(None) => ("current", None),
        // A repository without a published release returns 404. Do not claim it is up to date.
        Err(tauri_plugin_updater::Error::ReleaseNotFound) => ("unpublished", None),
        Err(error) => return Err(error.to_string()),
    };
    let info = UpdateInfo {
        status,
        version: update.as_ref().map(|u| u.version.clone()),
        notes: update.as_ref().and_then(|u| u.body.clone()),
        install_blocked: install_blocked(),
    };
    *state.pending.lock().map_err(|e| e.to_string())? = update;
    Ok(info)
}

#[derive(Clone, Serialize)]
pub(super) struct UpdateProgress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

struct Installing<'a>(&'a AtomicBool);
impl Drop for Installing<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub(super) async fn update_open_release() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        open::that("https://github.com/spechurkin/maploom/releases/latest")
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub(super) async fn update_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, UpdateState>,
    version: String,
    progress: Channel<UpdateProgress>,
) -> Result<(), String> {
    if let Some(reason) = install_blocked() {
        return Err(reason.into());
    }
    if state.installing.swap(true, Ordering::AcqRel) {
        return Err("Update already running".into());
    }
    let _installing = Installing(&state.installing);
    let mut update = state
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("Check for updates first")?;
    if update.version != version {
        return Err("Check for updates first".into());
    }
    let editor = app
        .try_state::<DesktopState>()
        .ok_or("Editor is loading")?
        .inner()
        .clone();
    if editor.0.lock().map_err(|e| e.to_string())?.dirty() {
        return Err("Update save first".into());
    }
    update.timeout = Some(Duration::from_secs(30 * 60));
    let mut downloaded = 0;
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = progress.send(UpdateProgress {
                    phase: "downloading",
                    downloaded,
                    total,
                });
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())?;
    // download() verifies the signature. Keep the editor locked from the last dirty
    // check through installation and restart, including edits queued during download.
    tauri::async_runtime::spawn_blocking(move || {
        let editor = editor.0.lock().map_err(|e| e.to_string())?;
        if editor.dirty() {
            return Err("Update save first".into());
        }
        let _ = progress.send(UpdateProgress {
            phase: "installing",
            downloaded: bytes.len() as u64,
            total: Some(bytes.len() as u64),
        });
        update.install(bytes).map_err(|e| e.to_string())?;
        app.restart();
    })
    .await
    .map_err(|e| e.to_string())?
}
