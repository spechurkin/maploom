//! External appearance packages. No document/editor dependency belongs in this module.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

pub const API_VERSION: u32 = 1;
const MAX_SOURCE: u64 = 2 * 1024 * 1024;
const MAX_RESOURCE: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Dark,
    Light,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Variant {
    pub id: String,
    pub name: String,
    pub mode: ThemeMode,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Branding {
    pub name: String,
    pub logo: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Manifest {
    pub format: String,
    pub api_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mode: Option<ThemeMode>,
    #[serde(default)]
    pub variants: Vec<Variant>,
    #[serde(default)]
    pub stylesheet: Option<String>,
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default)]
    pub branding: Option<Branding>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Theme {
    /// The directory name is the identity, with no language/slug restriction.
    pub id: String,
    pub variant: Option<String>,
    pub manifest: Manifest,
}

#[derive(Serialize)]
pub struct Catalog {
    pub directory: String,
    pub themes: Vec<Theme>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct Package {
    pub theme: Theme,
    pub modules: BTreeMap<String, String>,
}

/// Beside the executable, never in AppData or an installer-owned resource tree.
pub fn directory() -> Result<PathBuf> {
    #[cfg(debug_assertions)]
    {
        Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("themes"))
    }
    #[cfg(not(debug_assertions))]
    {
        #[cfg(target_os = "linux")]
        if let Some(image) = std::env::var_os("APPIMAGE") {
            return beside_application(Path::new(&image));
        }
        installed_directory(&std::env::current_exe()?)
    }
}

pub fn installed_directory(executable: &Path) -> Result<PathBuf> {
    // DEB/RPM put the binary in a shared /usr/bin, but the application itself
    // (its resources) is /usr/lib/Maploom. Never create a shared /usr/bin/themes.
    if executable.parent() == Some(Path::new("/usr/bin")) {
        return Ok(PathBuf::from("/usr/lib/Maploom/themes"));
    }
    beside_application(executable)
}

pub fn beside_application(executable: &Path) -> Result<PathBuf> {
    // Keep themes outside a signed .app, so replacing/removing that bundle keeps them.
    if let Some(bundle) = executable
        .ancestors()
        .find(|p| p.extension().is_some_and(|x| x == "app"))
    {
        return Ok(bundle
            .parent()
            .context("Application has no parent directory")?
            .join("themes"));
    }
    Ok(executable
        .parent()
        .context("Application has no parent directory")?
        .join("themes"))
}

fn relative(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty() && !path.contains(['\\', ':', '\0']) && !path.starts_with('/'),
        "Expected a relative theme path"
    );
    ensure!(
        path.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".."),
        "Theme path escapes its directory"
    );
    ensure!(
        Path::new(path)
            .components()
            .all(|p| matches!(p, Component::Normal(_))),
        "Invalid theme path"
    );
    Ok(())
}

pub fn theme_root(root: &Path, id: &str) -> Result<PathBuf> {
    relative(id)?;
    ensure!(!id.contains('/'), "Invalid theme directory");
    let base = root.canonicalize()?;
    let folder = base.join(id).canonicalize()?;
    ensure!(
        folder.is_dir() && folder.starts_with(&base) && folder != base,
        "Theme directory escapes themes"
    );
    Ok(folder)
}

pub fn resolve(folder: &Path, path: &str) -> Result<PathBuf> {
    relative(path)?;
    let base = folder.canonicalize()?;
    let file = base.join(path).canonicalize()?;
    ensure!(
        file.starts_with(&base) && file.is_file(),
        "Theme file escapes its directory"
    );
    Ok(file)
}

fn text(folder: &Path, path: &str) -> Result<String> {
    let file = resolve(folder, path)?;
    ensure!(
        std::fs::metadata(&file)?.len() <= MAX_SOURCE,
        "Theme source exceeds 2 MiB"
    );
    Ok(std::fs::read_to_string(file)?
        .trim_start_matches('\u{feff}')
        .to_owned())
}

pub fn read_theme(root: &Path, id: &str) -> Result<Theme> {
    let folder = theme_root(root, id)?;
    let manifest: Manifest = serde_json::from_str(&text(&folder, "theme.json")?)?;
    ensure!(
        manifest.format == "maploom/theme" && manifest.api_version == API_VERSION,
        "Unsupported theme API version"
    );
    ensure!(
        !manifest.name.trim().is_empty()
            && manifest.name.len() <= 512
            && manifest.description.len() <= 4096,
        "Invalid theme name or description"
    );
    ensure!(
        manifest.stylesheet.is_some() || manifest.entry.is_some(),
        "Theme needs a stylesheet or entry"
    );
    ensure!(manifest.variants.len() <= 32, "Too many theme variants");
    if let Some(branding) = &manifest.branding {
        ensure!(
            !branding.name.trim().is_empty()
                && branding.name.len() <= 128
                && !branding.name.chars().any(char::is_control),
            "Invalid theme brand name"
        );
        ensure!(
            matches!(
                Path::new(&branding.logo)
                    .extension()
                    .and_then(|s| s.to_str()),
                Some("svg" | "png" | "webp" | "jpg" | "jpeg")
            ),
            "Theme brand logo must be an image"
        );
        resolve(&folder, &branding.logo)?;
    }
    let mut ids = std::collections::HashSet::new();
    for variant in &manifest.variants {
        ensure!(
            !variant.id.trim().is_empty()
                && variant.id.len() <= 128
                && !variant.id.chars().any(char::is_control)
                && ids.insert(&variant.id)
                && !variant.name.trim().is_empty()
                && variant.name.len() <= 512,
            "Invalid or duplicate theme variant"
        );
    }
    if let Some(path) = &manifest.stylesheet {
        ensure!(path.ends_with(".css"), "Theme stylesheet must be CSS");
        resolve(&folder, path)?;
    }
    if let Some(path) = &manifest.entry {
        ensure!(is_code(path), "Theme entry must be JS, JSX, TS or TSX");
        resolve(&folder, path)?;
    }
    Ok(Theme {
        id: id.into(),
        variant: None,
        manifest,
    })
}

impl Theme {
    fn with_variant(mut self, id: Option<&str>) -> Result<Self> {
        let variant = match id {
            Some(id) => Some(
                self.manifest
                    .variants
                    .iter()
                    .find(|variant| variant.id == id)
                    .context("Theme variant unavailable")?,
            ),
            None => self.manifest.variants.first(),
        };
        if let Some(variant) = variant {
            self.variant = Some(variant.id.clone());
            self.manifest.name = variant.name.clone();
            self.manifest.mode = Some(variant.mode.clone());
        }
        Ok(self)
    }
}

pub fn read_variant(root: &Path, id: &str, variant: Option<&str>) -> Result<Theme> {
    read_theme(root, id)?.with_variant(variant)
}

pub fn scan(root: &Path) -> Catalog {
    let mut catalog = Catalog {
        directory: root.to_string_lossy().into_owned(),
        themes: vec![],
        warnings: vec![],
    };
    let result = (|| -> Result<()> {
        std::fs::create_dir_all(root)
            .context("Cannot create themes beside the application; check directory permissions")?;
        let mut entries = std::fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if !entry.path().is_dir() {
                continue;
            }
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            match read_theme(root, &id) {
                Ok(theme) => {
                    if theme.manifest.variants.is_empty() {
                        catalog.themes.push(theme);
                    } else {
                        for variant in &theme.manifest.variants {
                            catalog
                                .themes
                                .push(theme.clone().with_variant(Some(&variant.id))?);
                        }
                    }
                }
                Err(error) => catalog.warnings.push(format!("{id}: {error:#}")),
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        catalog.warnings.push(format!("{error:#}"));
    }
    catalog
}

fn is_code(path: &str) -> bool {
    [".js", ".jsx", ".ts", ".tsx"]
        .iter()
        .any(|extension| path.ends_with(extension))
        && !path.ends_with(".d.ts")
}

pub fn package(root: &Path, id: &str) -> Result<Package> {
    package_variant(root, id, None)
}

pub fn package_variant(root: &Path, id: &str, variant: Option<&str>) -> Result<Package> {
    let theme = read_variant(root, id, variant)?;
    let folder = theme_root(root, id)?;
    let mut modules = BTreeMap::new();
    fn visit(
        base: &Path,
        dir: &Path,
        modules: &mut BTreeMap<String, String>,
        depth: usize,
        total: &mut usize,
    ) -> Result<()> {
        ensure!(depth <= 16, "Theme modules are nested too deeply");
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            // Do not traverse symlinks/junctions or dependency trees.
            if entry.file_type()?.is_symlink()
                || entry.file_name() == "node_modules"
                || entry.file_name() == ".git"
            {
                continue;
            }
            let path = entry.path();
            ensure!(
                path.canonicalize()?.starts_with(base),
                "Theme module escapes its directory"
            );
            if path.is_dir() {
                visit(base, &path, modules, depth + 1, total)?;
            } else {
                let relative = path
                    .strip_prefix(base)?
                    .to_string_lossy()
                    .replace('\\', "/");
                if is_code(&relative) || relative.ends_with(".json") {
                    let source = text(base, &relative)?;
                    *total += source.len();
                    ensure!(
                        modules.len() < 512 && *total <= 8 * 1024 * 1024,
                        "Theme modules exceed 512 files or 8 MiB"
                    );
                    modules.insert(relative, source);
                }
            }
        }
        Ok(())
    }
    if theme.manifest.entry.is_some() {
        visit(&folder, &folder, &mut modules, 0, &mut 0)?;
    }
    Ok(Package { theme, modules })
}

pub fn resource(root: &Path, id: &str, path: &str) -> Result<(Vec<u8>, &'static str)> {
    let file = resolve(&theme_root(root, id)?, path)?;
    ensure!(
        std::fs::metadata(&file)?.len() <= MAX_RESOURCE,
        "Theme resource exceeds 32 MiB"
    );
    let mime = match file
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "json" => "application/json",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    };
    Ok((std::fs::read(file)?, mime))
}

pub fn preview_stylesheet(root: &Path, id: &str) -> Result<String> {
    let theme = read_theme(root, id)?;
    match theme.manifest.stylesheet {
        Some(path) => text(&theme_root(root, id)?, &path),
        None => Ok(String::new()),
    }
}

/// The installer never owns themes. The optional templates are created only on request,
/// in fresh directories; existing themes are never overwritten.
pub fn create_example(root: &Path) -> Result<Vec<String>> {
    std::fs::create_dir_all(root)?;
    let mut created = Vec::new();
    for (name, manifest, stylesheet, entry) in [
        (
            "Monokai",
            include_bytes!("../../resources/theme-template/monokai/theme.json").as_slice(),
            include_bytes!("../../resources/theme-template/monokai/style.css").as_slice(),
            None,
        ),
        (
            "GitHub",
            include_bytes!("../../resources/theme-template/github/theme.json").as_slice(),
            include_bytes!("../../resources/theme-template/github/style.css").as_slice(),
            None,
        ),
        (
            "Atlas",
            include_bytes!("../../resources/theme-template/atlas/theme.json").as_slice(),
            include_bytes!("../../resources/theme-template/atlas/style.css").as_slice(),
            Some(include_bytes!("../../resources/theme-template/atlas/theme.tsx").as_slice()),
        ),
        (
            "Contour",
            include_bytes!("../../resources/theme-template/contour/theme.json").as_slice(),
            include_bytes!("../../resources/theme-template/contour/style.css").as_slice(),
            None,
        ),
    ] {
        let mut number = 1;
        let (id, folder) = loop {
            let id = if number == 1 {
                name.to_owned()
            } else {
                format!("{name} {number}")
            };
            let folder = root.join(&id);
            match std::fs::create_dir(&folder) {
                Ok(()) => break (id, folder),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => number += 1,
                Err(error) => return Err(error.into()),
            }
        };
        let files = [("theme.json", manifest), ("style.css", stylesheet)];
        for (file, bytes) in files
            .into_iter()
            .chain(entry.map(|bytes| ("theme.tsx", bytes)))
        {
            use std::io::Write;
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(folder.join(file))?
                .write_all(bytes)?;
        }
        created.push(id);
    }
    Ok(created)
}
