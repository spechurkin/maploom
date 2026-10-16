//! Application-owned directories; projects only carry stable catalog identifiers.
use crate::{
    assets::{self, IndexedPack, Library, Source},
    document::{CatalogRef, Project},
    preferences::AssetDirectory,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Catalog {
    pub id: String,
    pub root: PathBuf,
    pub library: Library,
    pub loose_images: Library,
    pub packs: Vec<IndexedPack>,
    pub warnings: Vec<String>,
}

pub fn directory(path: &Path, id: Option<String>) -> Result<AssetDirectory> {
    let path = path.canonicalize()?;
    ensure!(path.is_dir(), "Select an asset directory");
    Ok(AssetDirectory {
        id: id.unwrap_or_else(crate::document::id),
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: path.to_string_lossy().into_owned(),
    })
}

pub fn scan(directory: &AssetDirectory) -> Catalog {
    let mut catalog = Catalog {
        id: directory.id.clone(),
        root: PathBuf::from(&directory.path),
        library: Library::new(),
        loose_images: Library::new(),
        packs: vec![],
        warnings: vec![],
    };
    let root = Path::new(&directory.path);
    let mut pending = vec![root.to_path_buf()];
    let mut visited = HashSet::new();
    let mut files = vec![];
    while let Some(path) = pending.pop() {
        let result = (|| -> Result<()> {
            if !visited.insert(path.canonicalize()?) {
                return Ok(());
            }
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                // Do not follow junctions/symlinks into other directory trees.
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    pending.push(entry.path());
                } else if kind.is_file() {
                    files.push(entry.path());
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            catalog.warnings.push(format!("{}: {e:#}", path.display()));
        }
    }
    files.sort();
    let packs: Vec<PathBuf> = files.iter().filter(|path| is_pack(path)).cloned().collect();
    for (path, indexed) in assets::index_packs(&packs) {
        match indexed {
            Ok(pack) => {
                catalog
                    .warnings
                    .extend(pack.warnings.iter().map(|w| format!("{}: {w}", pack.name)));
                for asset in &pack.assets {
                    catalog.insert(asset.clone());
                }
                catalog.packs.push(pack);
            }
            Err(e) => catalog.warnings.push(format!("{}: {e:#}", path.display())),
        }
    }
    for path in files
        .into_iter()
        .filter(|p| assets::is_image(&p.to_string_lossy()))
    {
        let result = (|| -> Result<_> {
            let mut asset = assets::import_image(&path)?;
            let relative = path
                .strip_prefix(root)
                .context("Image outside catalog")?
                .to_string_lossy()
                .replace('\\', "/");
            asset.meta.id = if relative.starts_with("textures/") || relative.starts_with("packs/") {
                format!("res://{relative}")
            } else {
                format!("catalog:{}/{relative}", directory.id)
            };
            asset.meta.path = relative.clone();
            asset.meta.tags = relative
                .split('/')
                .rev()
                .skip(1)
                .take(3)
                .map(str::to_owned)
                .collect();
            asset.source = Source::File(path.clone());
            Ok(asset)
        })();
        match result {
            Ok(asset) => {
                catalog
                    .loose_images
                    .insert(asset.meta.id.clone(), asset.clone());
                catalog.insert(asset);
            }
            Err(e) => catalog.warnings.push(format!("{}: {e:#}", path.display())),
        }
    }
    catalog
}
pub fn is_pack(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        matches!(
            ext.to_string_lossy().to_lowercase().as_str(),
            "dungeondraft_pack" | "mloom_fabric" | "mloon_fabric" | "mloom_prefab"
        )
    })
}
impl Catalog {
    pub fn pack_key(&self, pack: &IndexedPack) -> String {
        pack.path
            .strip_prefix(&self.root)
            .unwrap_or(&pack.path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn external(&self, mut asset: assets::Asset) -> assets::Asset {
        asset.source = Source::Catalog {
            id: self.id.clone(),
            source: Box::new(asset.source),
        };
        asset
    }

    fn insert(&mut self, asset: assets::Asset) {
        let id = asset.meta.id.clone();
        if self.library.contains_key(&id) {
            self.warnings
                .push(format!("Duplicate asset {id}; first file is used"));
            return;
        }
        let asset = self.external(asset);
        self.library.insert(id, asset);
    }
}
pub fn used_assets(project: &Project) -> HashSet<String> {
    let mut used: HashSet<String> = project
        .background_asset
        .iter()
        .cloned()
        .chain(
            project
                .levels
                .iter()
                .flat_map(|l| &l.layers)
                .flat_map(|l| &l.items)
                .flat_map(|i| i.asset_ids().into_iter().map(str::to_owned)),
        )
        .collect();
    for variation in &project.variations {
        if Some(&variation.id) == project.active_variation.as_ref() {
            continue;
        }
        used.extend(variation.background_asset.iter().cloned());
        used.extend(
            variation
                .levels
                .iter()
                .flat_map(|l| &l.layers)
                .flat_map(|l| &l.items)
                .flat_map(|i| i.asset_ids().into_iter().map(str::to_owned)),
        );
    }
    used
}

pub fn load_configured(loaded: &mut crate::formats::Loaded, dungeondraft: bool) {
    let preferences = crate::preferences::Preferences::load();
    let catalogs: Vec<_> = preferences.asset_directories.iter().map(scan).collect();
    if dungeondraft {
        select_for_dd(
            &mut loaded.project,
            &preferences.asset_directories,
            &catalogs,
        );
    }
    loaded
        .warnings
        .extend(resolve(&mut loaded.project, &mut loaded.library, &catalogs));
}

pub fn select_for_dd(project: &mut Project, directories: &[AssetDirectory], catalogs: &[Catalog]) {
    if !project.extensions.contains_key("dungeondraft_source") || !project.asset_catalogs.is_empty()
    {
        return;
    }
    let wanted = used_assets(project);
    let source = &project.extensions["dungeondraft_source"];
    let header = &source["header"]["asset_manifest"];
    for catalog in catalogs {
        let mut packs: Vec<_> = catalog
            .packs
            .iter()
            .filter(|pack| {
                (!pack.metadata.id.is_empty()
                    && header.as_array().is_some_and(|list| {
                        list.iter()
                            .any(|entry| entry["id"].as_str() == Some(&pack.metadata.id))
                    }))
                    || pack
                        .assets
                        .iter()
                        .any(|asset| wanted.contains(&asset.meta.id))
            })
            .map(|pack| catalog.pack_key(pack))
            .collect();
        if wanted
            .iter()
            .any(|id| catalog.loose_images.contains_key(id))
        {
            packs.push(String::new());
        }
        if !packs.is_empty()
            && let Some(dir) = directories.iter().find(|dir| dir.id == catalog.id)
        {
            project.asset_catalogs.push(CatalogRef {
                id: dir.id.clone(),
                name: dir.name.clone(),
                packs: Some(packs),
            });
        }
    }
}

/// Rebuild the active library so switching projects and undo cannot leak catalog assets.
pub fn resolve(project: &mut Project, library: &mut Library, catalogs: &[Catalog]) -> Vec<String> {
    library.retain(|_, asset| !matches!(asset.source, Source::Catalog { .. }));
    let mut warnings = vec![];
    for selected in &project.asset_catalogs {
        if let Some(catalog) = catalogs.iter().find(|c| c.id == selected.id) {
            let enabled = |key: &str| {
                selected
                    .packs
                    .as_ref()
                    .is_none_or(|packs| packs.iter().any(|p| p == key))
            };
            if let Some(packs) = &selected.packs {
                for key in packs {
                    if !key.is_empty()
                        && !catalog
                            .packs
                            .iter()
                            .any(|pack| catalog.pack_key(pack) == *key)
                    {
                        warnings.push(format!("Missing asset pack: {key} in {}", selected.name));
                    }
                }
            }
            let pack_assets = catalog
                .packs
                .iter()
                .filter(|pack| enabled(&catalog.pack_key(pack)))
                .flat_map(|pack| &pack.assets);
            let loose = catalog.loose_images.values().filter(|_| enabled(""));
            for asset in pack_assets.chain(loose) {
                let id = &asset.meta.id;
                if library
                    .get(id)
                    .is_some_and(|existing| matches!(existing.source, Source::Catalog { .. }))
                {
                    warnings.push(format!(
                        "Duplicate asset {id} in {}; first catalog is used",
                        selected.name
                    ));
                }
                library
                    .entry(id.clone())
                    .or_insert_with(|| catalog.external(asset.clone()));
            }
        } else {
            warnings.push(format!(
                "Missing asset catalog: {}. Locate its directory in Asset catalogs.",
                selected.name
            ));
        }
    }
    warnings.extend(resolve_references(project, library));
    warnings
}

pub fn resolve_references(project: &mut Project, library: &Library) -> Vec<String> {
    repair_legacy_path_sizes(project, library);
    let mut warnings = vec![];
    let wanted = used_assets(project);
    for id in &wanted {
        if let Some(asset) = library.get(id) {
            if let Source::Catalog { id: catalog, .. } = &asset.source {
                project
                    .assets
                    .entry(id.clone())
                    .and_modify(|meta| {
                        meta.path = asset.meta.path.clone();
                        meta.sha256 = asset.meta.sha256.clone();
                    })
                    .or_insert_with(|| asset.meta.clone());
                project.external_assets.insert(id.clone(), catalog.clone());
            }
        } else {
            warnings.push(format!("Missing asset: {id}"));
        }
    }
    if let Err(e) = crate::formats::resolve_dd_assets(project, library) {
        warnings.push(format!("{e:#}"));
    }
    warnings
}

// Earlier PCK indexes used 1x1 for every image. Repair automatic path repeats
// once, using the saved placeholder as the marker; keep explicit repeat edits.
fn repair_legacy_path_sizes(project: &mut Project, library: &Library) {
    fn is_pck(source: &Source) -> bool {
        match source {
            Source::Catalog { source, .. } => is_pck(source),
            Source::Pck { .. } => true,
            _ => false,
        }
    }
    let corrected: std::collections::BTreeMap<_, _> = project
        .assets
        .iter()
        .filter_map(|(id, meta)| {
            let asset = library.get(id)?;
            (meta.size == crate::document::Point::new(1., 1.)
                && asset.meta.size != meta.size
                && assets::category(&asset.meta) == "paths"
                && is_pck(&asset.source))
            .then(|| (id.clone(), asset.meta.size))
        })
        .collect();
    if corrected.is_empty() {
        return;
    }
    for level in project.all_levels_mut() {
        for item in level.layers.iter_mut().flat_map(|l| &mut l.items) {
            if let crate::document::ItemKind::Path { width, .. } = &item.kind {
                if let Some(size) = item.texture.as_ref().and_then(|id| corrected.get(id)) {
                    if (item.texture_scale - width).abs() <= 1e-6 {
                        item.texture_scale = width * size.x / size.y;
                    }
                }
            }
        }
    }
    for (id, size) in corrected {
        project.assets.get_mut(&id).unwrap().size = size;
    }
}
