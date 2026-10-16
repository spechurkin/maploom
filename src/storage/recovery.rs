//! Per-project, atomic recovery archives. Slot 1 is always the newest version.
use crate::{
    assets::{self, Library, Source},
    document::Project,
    formats,
};
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

pub const INTERVAL: Duration = Duration::from_secs(5 * 60);
pub const VERSIONS: usize = 5;

#[derive(Clone, Debug, Serialize)]
pub struct Backup {
    pub path: PathBuf,
    pub name: String,
    pub modified_ms: u64,
}

pub fn root() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Maploom/backups")
}

fn project_dir(root: &Path, id: &str) -> PathBuf {
    // Imported IDs need not be UUIDs and must never become filesystem paths.
    root.join(assets::hash(id.as_bytes()))
}

fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .take(100)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() {
        "Map".into()
    } else {
        name.into()
    }
}

fn slot(path: &Path) -> Option<usize> {
    let (name, index) = path.file_stem()?.to_str()?.rsplit_once("-recovery-")?;
    let index: usize = index.parse().ok()?;
    (!name.is_empty() && path.extension()? == "mloom" && (1..=VERSIONS).contains(&index))
        .then_some(index)
}

fn archives(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut paths = vec![];
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && slot(&entry.path()).is_some() {
            paths.push(entry.path());
        }
    }
    paths.sort_by_key(|p| slot(p));
    Ok(paths)
}

fn copy_atomic(from: &Path, to: &Path) -> Result<()> {
    assets::atomic_write(to, |file| {
        let mut source = fs::File::open(from)?;
        let modified = source.metadata()?.modified()?;
        std::io::copy(&mut source, file)?;
        file.set_modified(modified)?;
        Ok(())
    })
}

pub fn save(root: &Path, project: &Project, library: &Library) -> Result<PathBuf> {
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writer = WRITER
        .lock()
        .map_err(|_| anyhow::anyhow!("Recovery writer was interrupted"))?;
    let dir = project_dir(root, &project.id);
    fs::create_dir_all(&dir)?;
    let previous = archives(&dir)?;
    // Finish serialization and asset reads before touching any existing archive.
    let staging = tempfile::tempdir_in(&dir)?;
    let staged = staging.path().join("backup.mloom");
    formats::save(&staged, project, library)?;
    let name = safe_name(&project.name);
    let target = |n| dir.join(format!("{name}-recovery-{n}.mloom"));
    for source in previous.iter().rev() {
        let index = slot(source).unwrap();
        if index < VERSIONS {
            copy_atomic(source, &target(index + 1))?;
        }
    }
    copy_atomic(&staged, &target(1))?;
    // A rename changes the readable prefix, but never starts another history.
    for path in previous {
        if !(1..=VERSIONS).any(|n| path == target(n)) {
            fs::remove_file(path)?;
        }
    }
    Ok(target(1))
}

pub fn latest(root: &Path, project_id: Option<&str>) -> Result<Option<Backup>> {
    let mut paths = vec![];
    if let Some(id) = project_id {
        paths = archives(&project_dir(root, id))?;
    } else if root.exists() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                paths.extend(archives(&entry.path())?);
            }
        }
        // Only each project's first usable slot can compete as its newest copy.
        let mut newest = vec![];
        let mut dirs = std::collections::HashSet::new();
        for path in paths {
            if dirs.contains(path.parent().unwrap()) {
                continue;
            }
            if formats::load(&path).is_ok() {
                dirs.insert(path.parent().unwrap().to_path_buf());
                newest.push(path);
            }
        }
        paths = newest;
        paths.sort_by_key(|p| std::cmp::Reverse(fs::metadata(p).and_then(|m| m.modified()).ok()));
    }
    for path in paths {
        if let Ok(loaded) = formats::load(&path) {
            let modified_ms = fs::metadata(&path)?
                .modified()?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            return Ok(Some(Backup {
                path,
                name: loaded.project.name,
                modified_ms,
            }));
        }
    }
    Ok(None)
}

pub fn load(path: &Path) -> Result<formats::Loaded> {
    let mut loaded = formats::load(path).context("Could not open recovery archive")?;
    // Rotation may replace the archive while this document remains open.
    for asset in loaded.library.values_mut() {
        asset.source = Source::Bytes(Arc::new(assets::verified_bytes(asset)?));
    }
    Ok(loaded)
}
