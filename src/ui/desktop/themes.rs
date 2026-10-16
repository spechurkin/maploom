use crate::platform::themes;
use tauri::http::{Request, Response};

#[tauri::command]
pub async fn theme_catalog() -> Result<themes::Catalog, String> {
    tauri::async_runtime::spawn_blocking(|| themes::directory().map(|root| themes::scan(&root)))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn theme_package(id: String, variant: Option<String>) -> Result<themes::Package, String> {
    tauri::async_runtime::spawn_blocking(move || {
        themes::package_variant(&themes::directory()?, &id, variant.as_deref())
    })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn theme_preview(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        themes::preview_stylesheet(&themes::directory()?, &id)
    })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn theme_open_directory() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| -> anyhow::Result<()> {
        let root = themes::directory()?;
        std::fs::create_dir_all(&root)?;
        // Open Explorer directly, without starting PowerShell or waiting for Explorer to exit.
        #[cfg(target_os = "windows")]
        open::with_command(root, "explorer.exe").spawn()?;
        #[cfg(not(target_os = "windows"))]
        open::that(root)?;
        Ok(())
    })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
pub async fn theme_create_example() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| themes::create_example(&themes::directory()?))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("{e:#}"))
}

pub fn resource(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let result = (|| -> anyhow::Result<_> {
        anyhow::ensure!(request.method() == "GET", "Read-only theme resources");
        let path =
            percent_encoding::percent_decode_str(request.uri().path().trim_start_matches('/'))
                .decode_utf8()?;
        let (id, file) = path
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("Missing theme path"))?;
        themes::resource(&themes::directory()?, id, file)
    })();
    let (status, body, mime) = match result {
        Ok((bytes, mime)) => (200, bytes, mime),
        Err(_) => (404, b"Theme resource unavailable".to_vec(), "text/plain"),
    };
    Response::builder()
        .status(status)
        .header("Content-Type", mime)
        .header("Access-Control-Allow-Origin", "*")
        .header("Cache-Control", "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .header("Content-Security-Policy", "default-src 'none'; sandbox")
        .body(body)
        .expect("Static theme response headers")
}
