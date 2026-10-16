//! Tauri owns the desktop window. The Rust document remains authoritative.
use crate::{
    assets::{self, Library},
    catalog::{self, Catalog},
    document::*,
    editing, formats,
    gpu_scene::{GpuScene, SceneOptions, SceneRenderer},
    localization::{Locale, Locales},
    preferences::Preferences,
    render::{self, ExportOptions},
    storage::prefabs::{self, Prefab, Store},
    storage::recovery,
};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io::Cursor,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};

mod themes;
mod updates;

#[derive(Clone)]
pub struct DesktopState(Arc<Mutex<Editor>>, Arc<Mutex<SceneRenderer>>);

// Construction is cheap; the first asynchronous snapshot prepares the welcome
// screen. External catalog scans wait for opening a project or library request.
#[derive(Clone)]
struct EditorStartup {
    demo: bool,
    path: Option<PathBuf>,
    state: Arc<OnceLock<std::result::Result<DesktopState, String>>>,
}
impl EditorStartup {
    fn new(demo: bool, path: Option<PathBuf>) -> Self {
        Self {
            demo,
            path,
            state: Arc::new(OnceLock::new()),
        }
    }
    fn initialize(&self) -> std::result::Result<DesktopState, String> {
        self.state
            .get_or_init(|| {
                Editor::new(self.demo, self.path.clone())
                    .map(|editor| {
                        DesktopState(
                            Arc::new(Mutex::new(editor)),
                            Arc::new(Mutex::new(SceneRenderer::default())),
                        )
                    })
                    .map_err(|e| format!("{e:#}"))
            })
            .clone()
    }
}

#[cfg(test)]
mod startup_tests {
    use super::*;

    #[test]
    fn construction_defers_io_and_background_failures_are_returned_to_the_loading_screen() {
        let dir = tempfile::tempdir().unwrap();
        let startup = EditorStartup::new(false, Some(dir.path().join("missing.mloom")));
        assert!(
            startup.state.get().is_none(),
            "window setup must not read project/catalog files"
        );
        let worker = startup.clone();
        let error = std::thread::spawn(move || worker.initialize().err().unwrap())
            .join()
            .unwrap();
        assert!(!error.is_empty());
        assert!(startup.state.get().unwrap().is_err());
        assert_eq!(startup.initialize().err().unwrap(), error);
    }

    #[test]
    fn welcome_defers_libraries_until_use_and_reuses_the_loaded_catalogs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("catalog");
        std::fs::create_dir(&root).unwrap();
        let directory = catalog::directory(&root, None).unwrap();
        let preferences = Preferences {
            asset_directories: vec![directory],
            ..Preferences::default()
        };
        let mut editor = Editor::with_preferences(false, None, preferences).unwrap();
        let welcome = editor.snapshot(true);
        assert!(!welcome.project_open);
        assert!(!editor.resources_loaded);
        assert!(welcome.assets.unwrap().is_empty());
        assert!(welcome.catalogs.unwrap().is_empty());
        assert!(welcome.prefabs.is_empty());
        assert_eq!(welcome.preferences.asset_directories.len(), 1);

        // Created after startup: its presence proves indexing happens on demand.
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
            .save(root.join("object.png"))
            .unwrap();
        assert!(editor.apply(Action::LoadLibrary).unwrap());
        assert!(editor.resources_loaded);
        assert_eq!(editor.catalogs[0].library.len(), 1);
        assert!(!editor.default_ids.is_empty());
        assert!(!editor.prefabs.defaults.is_empty());
        std::fs::remove_file(root.join("object.png")).unwrap();
        editor.apply(Action::LoadLibrary).unwrap();
        assert_eq!(
            editor.catalogs[0].library.len(),
            1,
            "reopening the browser must reuse its index"
        );
        editor
            .apply(Action::New {
                name: "New map".into(),
                width: 16,
                height: 12,
                demo: false,
            })
            .unwrap();
        assert!(editor.project_open);
        assert!(!editor.dirty());
        assert!(
            editor
                .project
                .background_asset
                .as_ref()
                .is_some_and(|id| editor.library.contains_key(id))
        );
        assert_eq!(editor.catalogs[0].library.len(), 1);
        editor.apply(Action::RefreshDirectories).unwrap();
        assert!(
            editor.catalogs[0].library.is_empty(),
            "explicit rescan must refresh the index"
        );
    }

    #[test]
    fn opening_from_welcome_and_direct_launch_load_the_same_assets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("map.mloom");
        let mut project = Project::default();
        let default_id = project.background_asset.clone().unwrap();
        // An embedded copy must retain precedence over the standard pack.
        let image_path = dir.path().join("embedded.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([42; 4]))
            .save(&image_path)
            .unwrap();
        let mut asset = assets::import_image(&image_path).unwrap();
        asset.meta.id = default_id.clone();
        project
            .assets
            .insert(default_id.clone(), asset.meta.clone());
        let library = [(default_id.clone(), asset)].into_iter().collect();
        formats::save(&path, &project, &library).unwrap();
        let mut editor = Editor::with_preferences(false, None, Preferences::default()).unwrap();
        editor
            .apply(Action::Open {
                path: Some(path.to_string_lossy().into()),
            })
            .unwrap();
        let direct = Editor::with_preferences(false, Some(path), Preferences::default()).unwrap();
        assert!(editor.resources_loaded && direct.resources_loaded);
        assert_eq!(editor.library.len(), direct.library.len());
        assert_eq!(
            editor.library[&default_id].meta.sha256,
            library[&default_id].meta.sha256
        );
        assert_eq!(
            direct.library[&default_id].meta.sha256,
            library[&default_id].meta.sha256
        );
        assert_eq!(editor.project, direct.project);
        assert!(!editor.dirty() && !direct.dirty());
    }

    #[test]
    fn new_from_welcome_loads_standard_assets_before_choosing_the_background() {
        let mut editor = Editor::with_preferences(false, None, Preferences::default()).unwrap();
        assert!(!editor.resources_loaded);
        editor
            .apply(Action::New {
                name: "New map".into(),
                width: 16,
                height: 12,
                demo: false,
            })
            .unwrap();
        assert!(editor.resources_loaded && editor.project_open);
        let background = editor.project.background_asset.as_ref().unwrap();
        assert!(editor.default_ids.contains(background));
        assert!(editor.library.contains_key(background));
        assert!(!editor.dirty());
    }

    #[test]
    fn new_and_demo_skip_external_scans_then_library_loads_and_reuses_catalogs() {
        let dir = tempfile::tempdir().unwrap();
        let directory = catalog::directory(dir.path(), None).unwrap();
        image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
            .save(dir.path().join("object.png"))
            .unwrap();
        let preferences = Preferences {
            asset_directories: vec![directory],
            ..Preferences::default()
        };
        for demo in [false, true] {
            let mut editor = Editor::with_preferences(false, None, preferences.clone()).unwrap();
            editor
                .apply(Action::New {
                    name: "New map".into(),
                    width: 16,
                    height: 12,
                    demo,
                })
                .unwrap();
            assert!(editor.project_open && editor.resources_loaded);
            assert!(!editor.dirty());
            assert!(
                editor.catalogs.is_empty(),
                "map creation must not scan external catalogs"
            );
            assert!(editor.snapshot(true).catalogs.unwrap().is_empty());
            assert_eq!(editor.preferences.asset_directories.len(), 1);
            assert!(editor.project.asset_catalogs.is_empty());

            editor.apply(Action::LoadLibrary).unwrap();
            assert_eq!(editor.catalogs.len(), 1);
            assert_eq!(editor.catalogs[0].library.len(), 1);
            // A later file must only appear after an explicit rescan.
            image::RgbaImage::from_pixel(2, 2, image::Rgba([42; 4]))
                .save(dir.path().join("later.png"))
                .unwrap();
            editor.apply(Action::LoadLibrary).unwrap();
            assert_eq!(editor.catalogs[0].library.len(), 1);
            editor.apply(Action::RefreshDirectories).unwrap();
            assert_eq!(editor.catalogs[0].library.len(), 2);
            std::fs::remove_file(dir.path().join("later.png")).unwrap();
        }
    }

    #[test]
    #[ignore = "requires MAPLOOM_SETTINGS_DIR pointing to configured catalogs; read-only"]
    fn configured_new_map_and_first_scene_do_not_scan_external_catalogs() {
        let mut editor = Editor::new(false, None).unwrap();
        assert!(!editor.preferences.asset_directories.is_empty());
        let started = std::time::Instant::now();
        editor
            .apply(Action::New {
                name: "Startup probe".into(),
                width: 32,
                height: 24,
                demo: false,
            })
            .unwrap();
        let snapshot = serde_json::to_vec(&editor.snapshot(true)).unwrap();
        eprintln!(
            "New map and snapshot ({} bytes) in {:?}",
            snapshot.len(),
            started.elapsed()
        );
        assert!(editor.catalogs.is_empty());
        assert!(editor.project_open && !editor.dirty());
        let scene = SceneRenderer::default()
            .scene(
                &editor.project,
                &editor.resolved_library(),
                SceneOptions {
                    level: 0,
                    grid: true,
                    lights: true,
                    revision: editor.revision,
                    known_meshes: HashSet::new(),
                },
                &HashSet::new(),
            )
            .unwrap();
        assert_eq!(scene.revision, editor.revision);
        eprintln!("New map including first scene in {:?}", started.elapsed());
    }

    #[test]
    #[ignore = "requires MAPLOOM_SETTINGS_DIR pointing to configured catalogs; read-only"]
    fn configured_catalog_startup_completes_and_reuses_one_editor() {
        let startup = EditorStartup::new(false, None);
        let started = std::time::Instant::now();
        let state = startup.initialize().unwrap();
        let mut editor = state.0.lock().unwrap();
        let welcome = editor.snapshot(true);
        assert!(!welcome.project_open);
        assert!(welcome.assets.unwrap().is_empty());
        eprintln!("Welcome snapshot ready in {:?}", started.elapsed());
        assert!(!editor.resources_loaded);
        assert!(editor.catalogs.is_empty());
        let started = std::time::Instant::now();
        editor.apply(Action::LoadLibrary).unwrap();
        assert!(
            !editor.preferences.asset_directories.is_empty(),
            "set MAPLOOM_SETTINGS_DIR to the real settings directory"
        );
        let packs: usize = editor.catalogs.iter().map(|c| c.packs.len()).sum();
        let images: usize = editor.catalogs.iter().map(|c| c.library.len()).sum();
        eprintln!(
            "Deferred libraries: {} catalogs, {packs} packs, {images} images in {:?}",
            editor.catalogs.len(),
            started.elapsed()
        );
        assert_eq!(
            editor.catalogs.len(),
            editor.preferences.asset_directories.len()
        );
        assert!(!editor.snapshot(true).project_open);
        drop(editor);
        let repeated = startup.initialize().unwrap();
        assert!(Arc::ptr_eq(&state.0, &repeated.0));
    }
}
pub struct Editor {
    resources_loaded: bool,
    prefabs: Store,
    project_open: bool,
    recovered: bool,
    recovery: Option<recovery::Backup>,
    project: Project,
    library: Library,
    default_ids: HashSet<String>,
    history: History,
    revision: u64,
    saved: Project,
    path: Option<PathBuf>,
    warnings: Vec<String>,
    preferences: Preferences,
    locales: Locales,
    catalogs: Vec<Catalog>,
    catalog_warnings: Vec<String>,
}
#[derive(Serialize)]
pub struct AssetView {
    meta: AssetMeta,
    category: &'static str,
    builtin: bool,
}
#[derive(Serialize)]
pub struct CatalogView {
    directory: crate::preferences::AssetDirectory,
    packs: Vec<PackView>,
    assets: usize,
    loose_images: usize,
    warnings: Vec<String>,
}
#[derive(Serialize)]
pub struct PackView {
    key: String,
    path: String,
    name: String,
    file: String,
    assets: usize,
    metadata: assets::PackMetadata,
}
#[derive(Serialize)]
pub struct Snapshot {
    prefabs: Vec<PrefabView>,
    recovery: Option<recovery::Backup>,
    project_open: bool,
    project: Project,
    revision: u64,
    dirty: bool,
    path: Option<String>,
    undo: bool,
    redo: bool,
    assets: Option<Vec<AssetView>>,
    warnings: Vec<String>,
    preferences: Preferences,
    locales: Vec<Locale>,
    catalogs: Option<Vec<CatalogView>>,
}
#[derive(Serialize)]
pub struct PrefabView {
    key: String,
    source: &'static str,
    prefab: Prefab,
}

#[cfg(test)]
mod prefab_tests {
    use super::*;

    fn editor() -> Editor {
        let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
        let library: Library = pack
            .assets
            .into_iter()
            .map(|a| (a.meta.id.clone(), a))
            .collect();
        let project = Project {
            background_asset: None,
            ..Project::default()
        };
        Editor {
            resources_loaded: true,
            project_open: true,
            recovered: false,
            recovery: None,
            saved: project.clone(),
            project,
            default_ids: library.keys().cloned().collect(),
            library,
            prefabs: Store {
                defaults: pack.prefabs,
                ..Store::default()
            },
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
            catalog_warnings: vec![],
        }
    }

    #[test]
    fn placing_a_composition_is_one_undoable_edit_and_freezes_images_for_the_map() {
        let mut editor = editor();
        let view = editor.prefab_views().remove(0);
        let count = view.prefab.items.len();
        let before = editor.project.clone();
        editor
            .apply(Action::PlacePrefab {
                key: view.key,
                level: 0,
                layer: 1,
                position: Point::new(10., 10.),
                scale: 1.5,
                rotation: 0.3,
            })
            .unwrap();
        assert_eq!(editor.project.levels[0].layers[1].items.len(), 1);
        assert_eq!(
            editor.project.levels[0].layers[1].items[0]
                .prefab
                .as_ref()
                .unwrap()
                .items
                .len(),
            count
        );
        assert_eq!(editor.revision, 1);
        assert!(editor.dirty());
        for item in &editor.project.levels[0].layers[1].items {
            for id in item.asset_ids() {
                assert!(id.starts_with("prefab-asset:"));
                assert!(matches!(
                    editor.library[id].source,
                    assets::Source::Bytes(_)
                ));
            }
        }
        let placed = editor.project.clone();
        editor.apply(Action::Undo).unwrap();
        assert_eq!(editor.project, before);
        editor.apply(Action::Redo).unwrap();
        assert_eq!(editor.project, placed);
    }

    #[test]
    fn locked_and_hidden_targets_reject_placement_without_mutating_history() {
        let mut editor = editor();
        let key = editor.prefab_views().remove(0).key;
        for locked in [false, true] {
            editor.project.levels[0].layers[1].locked = locked;
            editor.project.levels[0].layers[1].visible = locked;
            let before = editor.project.clone();
            assert!(
                editor
                    .apply(Action::PlacePrefab {
                        key: key.clone(),
                        level: 0,
                        layer: 1,
                        position: Point::new(10., 10.),
                        scale: 1.,
                        rotation: 0.
                    })
                    .is_err()
            );
            assert_eq!(editor.project, before);
            assert!(!editor.history.can_undo());
            assert_eq!(editor.revision, 0);
        }
    }

    #[test]
    fn default_prefabs_follow_the_default_pack_toggle_and_personal_prefabs_are_independent() {
        let mut editor = editor();
        let personal = editor.prefabs.defaults[0].clone();
        editor.prefabs.personal.push(personal);
        assert!(editor.prefab_views().iter().any(|p| p.source == "default"));
        assert!(
            !editor.dirty(),
            "editing the prefab library does not change the map"
        );
        editor.project.use_default_assets = false;
        let views = editor.prefab_views();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].source, "user");
        assert!(!editor.history.can_undo());
    }
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    SavePrefab {
        level: usize,
        ids: HashSet<String>,
        name: String,
    },
    PlacePrefab {
        key: String,
        level: usize,
        layer: usize,
        position: Point,
        scale: f32,
        rotation: f32,
    },
    DeletePrefab {
        id: String,
    },
    ImportPrefabs,
    ExportPrefabs {
        keys: Vec<String>,
    },
    Commit {
        project: Box<Project>,
    },
    SelectVariation {
        id: String,
    },
    AddVariation {
        name: String,
    },
    RemoveVariation,
    RenameVariation {
        name: String,
    },
    ResizeMap {
        top: i32,
        right: i32,
        bottom: i32,
        left: i32,
    },
    New {
        name: String,
        width: u32,
        height: u32,
        demo: bool,
    },
    Open {
        path: Option<String>,
    },
    Save {
        save_as: bool,
    },
    AddDirectory {
        path: Option<String>,
        id: Option<String>,
    },
    RemoveDirectory {
        id: String,
    },
    LoadLibrary,
    RefreshDirectories,
    RegisterPaths {
        paths: Vec<String>,
    },
    Undo,
    Redo,
    Move {
        level: usize,
        ids: HashSet<String>,
        delta: Point,
    },
    Delete {
        level: usize,
        ids: HashSet<String>,
    },
    Duplicate {
        level: usize,
        ids: HashSet<String>,
    },
    Add {
        level: usize,
        layer: usize,
        item: Box<Item>,
    },
    Portal {
        level: usize,
        layer: usize,
        position: Point,
        asset: String,
        color: Color,
    },
    Preferences {
        preferences: Preferences,
    },
    Export {
        level: usize,
        format: String,
        pixels_per_grid: u32,
        grid: bool,
        lights: bool,
        quality: u8,
    },
    ExportAll {
        format: String,
        pixels_per_grid: u32,
        grid: bool,
        lights: bool,
        quality: u8,
    },
    ExportVariations {
        floors: bool,
        format: String,
        pixels_per_grid: u32,
        grid: bool,
        lights: bool,
        quality: u8,
    },
    Fabric {
        name: String,
        author: String,
        ids: Vec<String>,
    },
    Crop {
        asset: String,
        name: String,
        rect: [u32; 4],
        flip_x: bool,
        flip_y: bool,
        turns: u32,
    },
    Recover,
    Autosave,
}
impl Editor {
    /// Resolve only the requested image. Thumbnail/silhouette requests must not
    /// copy every pack while holding the editor lock.
    fn resolved_asset(&self, id: &str) -> Option<assets::Asset> {
        let mut asset = self.library.get(id)?.clone();
        if let Some(meta) = self.project.assets.get(id) {
            let source_identity = matches!(asset.source, assets::Source::Catalog { .. })
                .then(|| (asset.meta.path.clone(), asset.meta.sha256.clone()));
            asset.meta = meta.clone();
            if let Some((path, sha256)) = source_identity {
                asset.meta.path = path;
                asset.meta.sha256 = sha256;
            }
        }
        Some(asset)
    }

    fn resolved_library(&self) -> Library {
        let mut library = self.library.clone();
        for (id, meta) in &self.project.assets {
            if let Some(asset) = library.get_mut(id) {
                let source_identity = matches!(asset.source, assets::Source::Catalog { .. })
                    .then(|| (asset.meta.path.clone(), asset.meta.sha256.clone()));
                asset.meta = meta.clone();
                if let Some((path, sha256)) = source_identity {
                    asset.meta.path = path;
                    asset.meta.sha256 = sha256;
                }
            }
        }
        library
    }
    fn new(demo: bool, path: Option<PathBuf>) -> Result<Self> {
        Self::with_preferences(demo, path, Preferences::load())
    }
    fn with_preferences(
        demo: bool,
        path: Option<PathBuf>,
        mut preferences: Preferences,
    ) -> Result<Self> {
        let locales = Locales::startup();
        if !locales.catalogs.contains_key(&preferences.locale) {
            preferences.locale = "en".into();
        }
        let mut library = Library::new();
        let (mut project, mut warnings) = if let Some(p) = &path {
            let loaded = formats::load(p)?;
            library.extend(loaded.library);
            (loaded.project, loaded.warnings)
        } else {
            (
                if demo {
                    Project::demo()
                } else {
                    Project::default()
                },
                vec![],
            )
        };
        if path.is_none() {
            project.use_default_assets = preferences.default_assets;
        }
        let recovery = match recovery::latest(&recovery::root(), None) {
            Ok(backup) => backup,
            Err(error) => {
                warnings.push(format!("Recovery: {error:#}"));
                None
            }
        };
        let mut editor = Self {
            resources_loaded: false,
            project_open: demo || path.is_some(),
            recovered: false,
            recovery,
            saved: project.clone(),
            project,
            library,
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path,
            warnings,
            preferences,
            catalogs: vec![],
            catalog_warnings: vec![],
            locales,
        };
        if editor.project_open {
            editor.load_catalogs()?;
            editor.resolve_packs();
        }
        editor.warnings.extend(editor.locales.warnings.clone());
        Ok(editor)
    }
    /// New maps need standard assets and personal prefabs, but no external
    /// catalogs. Keep potentially large directory scans out of map creation.
    fn load_resources(&mut self) -> Result<()> {
        if self.resources_loaded {
            return Ok(());
        }
        let mut prefabs = Store::load(&prefabs::path())?;
        let mut library = Library::new();
        // One pass supplies both standard images and prefab definitions.
        if let Some(path) = assets::default_fabric_path() {
            let pack = assets::index_pack(&path)?;
            prefabs.defaults = pack.prefabs;
            library.extend(
                pack.assets
                    .into_iter()
                    .map(|asset| (asset.meta.id.clone(), asset)),
            );
        }
        let default_ids = library.keys().cloned().collect();
        // Publish only a complete successful load; a failed attempt can be retried.
        library.extend(std::mem::take(&mut self.library));
        self.library = library;
        self.default_ids = default_ids;
        self.prefabs = prefabs;
        self.resources_loaded = true;
        Ok(())
    }
    fn load_catalogs(&mut self) -> Result<()> {
        self.load_resources()?;
        for directory in &self.preferences.asset_directories {
            if !self.catalogs.iter().any(|c| c.id == directory.id) {
                self.catalogs.push(catalog::scan(directory));
            }
        }
        Ok(())
    }
    fn prefab_views(&self) -> Vec<PrefabView> {
        let mut result = vec![];
        if self.project.use_default_assets {
            result.extend(self.prefabs.defaults.iter().map(|p| PrefabView {
                key: format!("default:{}", p.id),
                source: "default",
                prefab: p.clone(),
            }));
        }
        result.extend(self.prefabs.personal.iter().map(|p| PrefabView {
            key: format!("user:{}", p.id),
            source: "user",
            prefab: p.clone(),
        }));
        for catalog in &self.catalogs {
            let Some(selected) = self
                .project
                .asset_catalogs
                .iter()
                .find(|c| c.id == catalog.id)
            else {
                continue;
            };
            for pack in &catalog.packs {
                let key = catalog.pack_key(pack);
                if selected.packs.as_ref().is_some_and(|p| !p.contains(&key)) {
                    continue;
                }
                result.extend(pack.prefabs.iter().map(|p| PrefabView {
                    key: format!("pack:{}/{}/{}", catalog.id, key, p.id),
                    source: "pack",
                    prefab: p.clone(),
                }));
            }
        }
        result
    }
    fn dirty(&self) -> bool {
        self.project_open && (self.recovered || self.project != self.saved)
    }
    fn snapshot(&self, library: bool) -> Snapshot {
        Snapshot {
            prefabs: self.prefab_views(),
            recovery: self.recovery.clone(),
            project_open: self.project_open,
            project: self.project.clone(),
            revision: self.revision,
            dirty: self.dirty(),
            path: self.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            undo: self.history.can_undo(),
            redo: self.history.can_redo(),
            assets: library.then(|| {
                self.resolved_library()
                    .values()
                    .map(|a| AssetView {
                        meta: a.meta.clone(),
                        category: if a.meta.id.starts_with("prefab-asset:")
                            || a.meta.id == crate::storage::dd_prefabs::BORDER
                        {
                            "prefab-images"
                        } else {
                            assets::category(&a.meta)
                        },
                        builtin: self.default_ids.contains(&a.meta.id),
                    })
                    .collect()
            }),
            warnings: self
                .warnings
                .iter()
                .chain(&self.catalog_warnings)
                .cloned()
                .collect(),
            preferences: self.preferences.clone(),
            locales: self.locales.catalogs.values().cloned().collect(),
            catalogs: library.then(|| {
                self.catalogs
                    .iter()
                    .filter_map(|c| {
                        self.preferences
                            .asset_directories
                            .iter()
                            .find(|d| d.id == c.id)
                            .map(|d| CatalogView {
                                directory: d.clone(),
                                assets: c.library.len(),
                                loose_images: c.loose_images.len(),
                                warnings: c.warnings.clone(),
                                packs: c
                                    .packs
                                    .iter()
                                    .map(|p| PackView {
                                        key: c.pack_key(p),
                                        path: p.path.to_string_lossy().into_owned(),
                                        name: p.name.clone(),
                                        file: p
                                            .path
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned(),
                                        assets: p.assets.len(),
                                        metadata: p.metadata.clone(),
                                    })
                                    .collect(),
                            })
                    })
                    .collect()
            }),
        }
    }
    fn resolve_packs(&mut self) {
        // Migrate legacy absolute pack paths into application-owned directories.
        let legacy = std::mem::take(&mut self.project.packs);
        for pack in legacy {
            if let Some(parent) = PathBuf::from(&pack.path).parent() {
                match self.register_directory(parent.to_path_buf(), None) {
                    Ok(id) => {
                        let key = PathBuf::from(&pack.path)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned();
                        if let Some(reference) =
                            self.project.asset_catalogs.iter_mut().find(|r| r.id == id)
                        {
                            if let Some(packs) = &mut reference.packs {
                                if !packs.contains(&key) {
                                    packs.push(key);
                                }
                            }
                        } else {
                            self.project.asset_catalogs.push(CatalogRef {
                                id,
                                name: pack.name,
                                packs: Some(vec![key]),
                            });
                        }
                    }
                    Err(e) => {
                        self.warnings.push(format!("{}: {e:#}", pack.name));
                        self.project.packs.push(pack);
                    }
                }
            }
        }
        if self.path.as_ref().is_some_and(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_map"))
        }) {
            catalog::select_for_dd(
                &mut self.project,
                &self.preferences.asset_directories,
                &self.catalogs,
            );
        }
        self.refresh_library();
        self.saved = self.project.clone();
    }
    fn refresh_library(&mut self) {
        self.library.extend(self.prefabs.assets.clone());
        self.catalog_warnings =
            catalog::resolve(&mut self.project, &mut self.library, &self.catalogs);
    }
    fn register_directory(&mut self, path: PathBuf, id: Option<String>) -> Result<String> {
        let mut dir = catalog::directory(&path, id.clone())?;
        if let Some(existing) = self
            .preferences
            .asset_directories
            .iter()
            .find(|d| PathBuf::from(&d.path) == PathBuf::from(&dir.path))
        {
            if id.as_ref().is_none_or(|id| id == &existing.id) {
                return Ok(existing.id.clone());
            }
            // A project from another computer may know this folder by a different
            // identifier. Keep both aliases so other projects retain their links.
        }
        // Retain the project identifier when locating a moved/missing directory.
        if let Some(reference) = self.project.asset_catalogs.iter().find(|r| r.id == dir.id) {
            dir.name = reference.name.clone();
        }
        let indexed = catalog::scan(&dir);
        let mut preferences = self.preferences.clone();
        preferences.asset_directories.retain(|d| d.id != dir.id);
        preferences.asset_directories.push(dir.clone());
        preferences.save()?;
        self.preferences = preferences;
        self.catalogs.retain(|c| c.id != dir.id);
        self.catalogs.push(indexed);
        Ok(dir.id)
    }
    pub fn commit(&mut self, mut project: Project) -> Result<()> {
        ensure!(
            self.project.active_variation == project.active_variation
                && self.project.variations == project.variations,
            "Use variation actions to change variations"
        );
        self.project.protect_main_floor(&mut project)?;
        editing::protect_locked(&self.project, &project)?;
        editing::reconcile_geometry(&self.project, &mut project);
        project.sync_portals();
        project.validate()?;
        project.migrate_terrain();
        project.sync_active_variation();
        project.validate()?;
        if project != self.project {
            let catalogs_changed = project.asset_catalogs != self.project.asset_catalogs;
            self.history.record(self.project.clone());
            self.project = project;
            if catalogs_changed {
                self.refresh_library();
            } else {
                self.catalog_warnings
                    .retain(|w| !w.starts_with("Missing asset:"));
                self.catalog_warnings.extend(catalog::resolve_references(
                    &mut self.project,
                    &self.library,
                ));
            }
            self.revision += 1;
        }
        Ok(())
    }
    fn confirm_replace(&self) -> Result<()> {
        if self.dirty() {
            let yes = rfd::MessageDialog::new()
                .set_title("Maploom")
                .set_description(if self.preferences.locale == "ru" {
                    "В карте есть несохранённые изменения. Продолжить без сохранения?"
                } else {
                    "Discard unsaved changes?"
                })
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            ensure!(yes == rfd::MessageDialogResult::Yes, "Cancelled");
        }
        Ok(())
    }
    fn apply(&mut self, action: Action) -> Result<bool> {
        ensure!(
            self.project_open
                || matches!(
                    &action,
                    Action::New { .. }
                        | Action::Open { .. }
                        | Action::Recover
                        | Action::Preferences { .. }
                        | Action::AddDirectory { .. }
                        | Action::RemoveDirectory { .. }
                        | Action::LoadLibrary
                        | Action::RefreshDirectories
                        | Action::RegisterPaths { .. }
                        | Action::Autosave
                ),
            "Create or open a project first"
        );
        match action {
            Action::SavePrefab { level, ids, name } => {
                let level = self.project.levels.get(level).context("Invalid level")?;
                let items: Vec<_> = level
                    .layers
                    .iter()
                    .filter(|l| l.visible)
                    .flat_map(|l| {
                        l.items.iter().filter(|i| ids.contains(&i.id)).map(|i| {
                            let mut i = i.clone();
                            i.tint[3] = (i.tint[3] as f32 * l.opacity).round() as u8;
                            i
                        })
                    })
                    .collect();
                ensure!(items.len() == ids.len(), "Invalid prefab selection");
                let prefab = Prefab::capture(name, items)?;
                let (mut captured, images) =
                    prefabs::portable(&[prefab], &self.resolved_library())?;
                let mut store = self.prefabs.clone();
                ensure!(store.personal.len() < 1000, "Too many prefabs");
                store.personal.append(&mut captured);
                store
                    .assets
                    .extend(images.into_iter().map(|a| (a.meta.id.clone(), a)));
                store.save(&prefabs::path())?;
                self.prefabs = store;
                self.library.extend(self.prefabs.assets.clone());
                return Ok(true);
            }
            Action::PlacePrefab {
                key,
                level,
                layer,
                position,
                scale,
                rotation,
            } => {
                let prefab = self
                    .prefab_views()
                    .into_iter()
                    .find(|p| p.key == key)
                    .context("Missing prefab")?
                    .prefab;
                let mut project = self.project.clone();
                let target = project
                    .levels
                    .get_mut(level)
                    .and_then(|l| l.layers.get_mut(layer))
                    .context("Invalid layer")?;
                ensure!(target.visible && !target.locked, "Layer is not editable");
                // Freeze dependencies in the map too: disabling a source pack must not break placed copies.
                let (copies, images) = prefabs::portable(&[prefab], &self.resolved_library())?;
                target
                    .items
                    .push(copies[0].instance(position, scale, rotation)?);
                let old_library = self.library.clone();
                self.library
                    .extend(images.into_iter().map(|a| (a.meta.id.clone(), a)));
                if let Err(error) = self.commit(project) {
                    self.library = old_library;
                    return Err(error);
                }
                return Ok(true);
            }
            Action::DeletePrefab { id } => {
                let mut store = self.prefabs.clone();
                ensure!(
                    store.personal.iter().any(|p| p.id == id),
                    "Missing personal prefab"
                );
                store.personal.retain(|p| p.id != id);
                store.save(&prefabs::path())?;
                self.prefabs = store;
                return Ok(true);
            }
            Action::ImportPrefabs => {
                let path = rfd::FileDialog::new()
                    .add_filter(
                        "Prefabs",
                        &["mloom_prefab", "dungeondraft_prefab", "mloom_fabric"],
                    )
                    .pick_file()
                    .context("Cancelled")?;
                let (mut incoming, mut library) = if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_prefab"))
                {
                    let mut library = self.resolved_library();
                    let prefab = crate::storage::dd_prefabs::import_file(&path, &mut library)?;
                    (vec![prefab], library)
                } else {
                    let pack = assets::index_pack(&path)?;
                    (
                        pack.prefabs,
                        pack.assets
                            .into_iter()
                            .map(|a| (a.meta.id.clone(), a))
                            .collect(),
                    )
                };
                ensure!(!incoming.is_empty(), "This pack contains no prefabs");
                for prefab in &mut incoming {
                    if prefab.preview.is_none()
                        && prefab
                            .items
                            .iter()
                            .any(|i| !matches!(i.kind, ItemKind::Object { .. }))
                    {
                        prefabs::add_preview(prefab, &mut library)?;
                    }
                }
                let (mut imported, images) = prefabs::portable(&incoming, &library)?;
                let mut store = self.prefabs.clone();
                for prefab in &mut imported {
                    prefab.id = id();
                }
                store.personal.append(&mut imported);
                store
                    .assets
                    .extend(images.into_iter().map(|a| (a.meta.id.clone(), a)));
                store.save(&prefabs::path())?;
                self.prefabs = store;
                self.library.extend(self.prefabs.assets.clone());
                return Ok(true);
            }
            Action::ExportPrefabs { keys } => {
                ensure!(!keys.is_empty(), "Select prefabs first");
                let views = self.prefab_views();
                let mut selected = keys
                    .iter()
                    .map(|key| {
                        views
                            .iter()
                            .find(|p| &p.key == key)
                            .map(|p| p.prefab.clone())
                            .context("Missing prefab")
                    })
                    .collect::<Result<Vec<_>>>()?;
                // Different packs may use the same local ID.
                for prefab in &mut selected {
                    prefab.id = id();
                }
                let (prefabs, images) = prefabs::portable(&selected, &self.resolved_library())?;
                let name = if prefabs.len() == 1 {
                    prefabs[0].name.clone()
                } else {
                    "Prefabs".into()
                };
                let path = rfd::FileDialog::new()
                    .set_file_name(format!("{}.mloom_prefab", render::safe_export_name(&name)))
                    .add_filter("Maploom prefabs", &["mloom_prefab"])
                    .save_file()
                    .context("Cancelled")?;
                assets::save_prefab_file(
                    &path.with_extension("mloom_prefab"),
                    &name,
                    &images,
                    &prefabs,
                )?;
            }
            action @ (Action::SelectVariation { .. }
            | Action::AddVariation { .. }
            | Action::RemoveVariation
            | Action::RenameVariation { .. }) => {
                let mut next = self.project.clone();
                match action {
                    Action::SelectVariation { id } => next.select_variation(&id)?,
                    Action::AddVariation { name } => next.add_variation(name)?,
                    Action::RemoveVariation => next.remove_variation()?,
                    Action::RenameVariation { name } => next.rename_variation(name)?,
                    _ => unreachable!(),
                }
                next.sync_active_variation();
                next.validate()?;
                if next != self.project {
                    self.history.record(self.project.clone());
                    self.project = next;
                    self.revision += 1;
                }
            }
            Action::ResizeMap {
                top,
                right,
                bottom,
                left,
            } => {
                let mut next = self.project.clone();
                editing::resize_map(&mut next, top, right, bottom, left)?;
                if next != self.project {
                    self.history.record(self.project.clone());
                    self.project = next;
                    self.revision += 1;
                }
            }
            Action::Commit { project } => {
                let changed = project.asset_catalogs != self.project.asset_catalogs;
                self.commit(*project)?;
                if changed {
                    return Ok(true);
                }
            }
            Action::Undo => {
                if self.history.undo(&mut self.project) {
                    self.refresh_library();
                    self.revision += 1;
                    return Ok(true);
                }
            }
            Action::Redo => {
                if self.history.redo(&mut self.project) {
                    self.refresh_library();
                    self.revision += 1;
                    return Ok(true);
                }
            }
            Action::Move { level, ids, delta } => {
                ensure!(level < self.project.levels.len(), "Invalid level");
                let mut p = self.project.clone();
                editing::move_items(&mut p, level, &ids, delta);
                self.commit(p)?;
            }
            Action::Delete { level, ids } => {
                ensure!(level < self.project.levels.len(), "Invalid level");
                let mut p = self.project.clone();
                editing::delete_items(&mut p, level, &ids);
                self.commit(p)?;
            }
            Action::Duplicate { level, ids } => {
                ensure!(level < self.project.levels.len(), "Invalid level");
                let mut p = self.project.clone();
                editing::duplicate_items(&mut p, level, &ids);
                self.commit(p)?;
            }
            Action::Add { level, layer, item } => {
                let mut p = self.project.clone();
                let target = p
                    .levels
                    .get_mut(level)
                    .and_then(|l| l.layers.get_mut(layer))
                    .context("Invalid layer")?;
                ensure!(!target.locked && target.visible, "Layer is not editable");
                if matches!(item.kind, ItemKind::Room { .. }) {
                    editing::add_room(&mut p, level, layer, *item)?;
                } else {
                    target.items.push(*item);
                }
                self.commit(p)?;
            }
            Action::Portal {
                level,
                layer,
                position,
                asset,
                color,
            } => {
                let library = self.resolved_library();
                let meta = &library.get(&asset).context("Missing door asset")?.meta;
                let item = editing::portal_item(&self.project, level, position, meta, color)?;
                return self.apply(Action::Add {
                    level,
                    layer,
                    item: Box::new(item),
                });
            }
            Action::New {
                name,
                width,
                height,
                demo,
            } => {
                self.confirm_replace()?;
                self.load_resources()?;
                let mut p = if demo {
                    Project::demo()
                } else {
                    Project {
                        name,
                        width,
                        height,
                        use_default_assets: self.preferences.default_assets,
                        ..Default::default()
                    }
                };
                if !demo
                    && p.background_asset
                        .as_ref()
                        .is_some_and(|id| !self.library.contains_key(id))
                {
                    p.background_asset = None;
                }
                p.validate()?;
                self.project_open = true;
                self.recovered = false;
                self.project = p;
                self.saved = self.project.clone();
                self.path = None;
                self.history = History::default();
                self.revision += 1;
                self.warnings.clear();
                self.library.retain(|id, _| self.default_ids.contains(id));
                self.refresh_library();
                return Ok(true);
            }
            Action::Open { path } => {
                let path = path
                    .map(PathBuf::from)
                    .or_else(|| {
                        rfd::FileDialog::new()
                            .add_filter(
                                "Maploom / Dungeondraft",
                                &["mloom", "json", "dungeondraft_map"],
                            )
                            .pick_file()
                    })
                    .context("Cancelled")?;
                self.confirm_replace()?;
                let loaded = formats::load(&path)?;
                self.load_catalogs()?;
                self.project_open = true;
                self.recovered = false;
                self.project = loaded.project;
                self.library.retain(|id, _| self.default_ids.contains(id));
                self.library.extend(loaded.library);
                self.path = Some(path);
                self.warnings = loaded.warnings;
                self.resolve_packs();
                self.history = History::default();
                self.revision += 1;
                return Ok(true);
            }
            Action::Save { save_as } => {
                let path = if save_as
                    || self.path.as_ref().is_some_and(|p| {
                        p.extension()
                            .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_map"))
                    }) {
                    None
                } else {
                    self.path.clone()
                }
                .or_else(|| {
                    rfd::FileDialog::new()
                        .set_file_name(format!(
                            "{}.mloom",
                            self.project.name.replace(['/', '\\', ':'], "-")
                        ))
                        .add_filter("Maploom", &["mloom"])
                        .save_file()
                })
                .context("Cancelled")?;
                formats::save(&path, &self.project, &self.resolved_library())?;
                self.path = Some(path);
                self.recovered = false;
                self.saved = self.project.clone();
            }
            Action::AddDirectory { path, id } => {
                let path = path
                    .map(PathBuf::from)
                    .or_else(|| rfd::FileDialog::new().pick_folder())
                    .context("Cancelled")?;
                self.load_catalogs()?;
                self.register_directory(path, id)?;
                self.refresh_library();
                return Ok(true);
            }
            Action::RemoveDirectory { id } => {
                let mut preferences = self.preferences.clone();
                preferences.asset_directories.retain(|d| d.id != id);
                preferences.save()?;
                self.preferences = preferences;
                self.catalogs.retain(|c| c.id != id);
                self.refresh_library();
                return Ok(true);
            }
            Action::RefreshDirectories => {
                self.load_resources()?;
                self.catalogs = self
                    .preferences
                    .asset_directories
                    .iter()
                    .map(catalog::scan)
                    .collect();
                self.refresh_library();
                return Ok(true);
            }
            Action::RegisterPaths { paths } => {
                self.load_catalogs()?;
                for path in paths.into_iter().map(PathBuf::from) {
                    let directory = if path.is_dir() {
                        path
                    } else {
                        path.parent()
                            .context("File has no directory")?
                            .to_path_buf()
                    };
                    self.register_directory(directory, None)?;
                }
                self.refresh_library();
                return Ok(true);
            }
            Action::LoadLibrary => {
                self.load_catalogs()?;
                self.refresh_library();
                return Ok(true);
            }
            Action::Preferences { preferences } => {
                ensure!(
                    self.locales.catalogs.contains_key(&preferences.locale),
                    "Unsupported locale"
                );
                if preferences.theme != self.preferences.theme
                    || preferences.theme_variant != self.preferences.theme_variant
                {
                    if let Some(id) = &preferences.theme {
                        crate::platform::themes::read_variant(
                            &crate::platform::themes::directory()?,
                            id,
                            preferences.theme_variant.as_deref(),
                        )?;
                    }
                }
                preferences.save()?;
                self.preferences = preferences;
            }
            Action::Export {
                level,
                format,
                pixels_per_grid,
                grid,
                lights,
                quality,
            } => {
                ensure!(
                    ["png", "webp", "jpeg", "svg", "dd2vtt"].contains(&format.as_str()),
                    "Unsupported export format"
                );
                let variation = self
                    .project
                    .variations
                    .iter()
                    .find(|v| Some(&v.id) == self.project.active_variation.as_ref())
                    .map(|v| v.name.as_str());
                let names = render::floor_export_names(&self.project, &format, variation);
                let file_name = names.get(level).context("Invalid export level")?;
                let path = rfd::FileDialog::new()
                    .set_file_name(file_name)
                    .add_filter("Map export", &[&format])
                    .save_file()
                    .context("Cancelled")?;
                render::export(
                    &path,
                    &self.project,
                    &self.resolved_library(),
                    ExportOptions {
                        level,
                        pixels_per_grid,
                        grid,
                        lights,
                        jpeg_quality: quality,
                    },
                )?;
            }
            Action::ExportAll {
                format,
                pixels_per_grid,
                grid,
                lights,
                quality,
            } => {
                ensure!(
                    ["png", "webp", "svg", "dd2vtt"].contains(&format.as_str()),
                    "Floor transparency requires PNG, WebP, SVG or VTT"
                );
                let path = rfd::FileDialog::new()
                    .set_file_name(format!(
                        "{}-floors.zip",
                        render::safe_export_name(&self.project.name)
                    ))
                    .add_filter("ZIP", &["zip"])
                    .save_file()
                    .context("Cancelled")?;
                render::export_all(
                    &path,
                    &self.project,
                    &self.resolved_library(),
                    ExportOptions {
                        level: self.project.main_level_index(),
                        pixels_per_grid,
                        grid,
                        lights,
                        jpeg_quality: quality,
                    },
                    &format,
                )?;
            }
            Action::ExportVariations {
                floors,
                format,
                pixels_per_grid,
                grid,
                lights,
                quality,
            } => {
                let options = ExportOptions {
                    level: 0,
                    pixels_per_grid,
                    grid,
                    lights,
                    jpeg_quality: quality,
                };
                render::validate_variation_export(&self.project, floors, &format)?;
                let name = render::safe_export_name(&self.project.name);
                let path = if floors {
                    let folder = rfd::FileDialog::new().pick_folder().context("Cancelled")?;
                    let paths = render::variation_archive_names(&self.project);
                    if paths.iter().any(|name| folder.join(name).exists()) {
                        let answer = rfd::MessageDialog::new()
                            .set_title("Maploom")
                            .set_description(if self.preferences.locale == "ru" {
                                "Некоторые ZIP-архивы уже существуют. Заменить их?"
                            } else {
                                "Some ZIP archives already exist. Replace them?"
                            })
                            .set_buttons(rfd::MessageButtons::YesNo)
                            .show();
                        ensure!(answer == rfd::MessageDialogResult::Yes, "Cancelled");
                    }
                    folder
                } else {
                    rfd::FileDialog::new()
                        .set_file_name(format!("{name}.zip"))
                        .add_filter("ZIP", &["zip"])
                        .save_file()
                        .context("Cancelled")?
                        .with_extension("zip")
                };
                render::export_variations(
                    &path,
                    &self.project,
                    &self.resolved_library(),
                    options,
                    &format,
                    floors,
                )?;
            }
            Action::Fabric { name, author, ids } => {
                ensure!(!ids.is_empty(), "Select assets first");
                let library = self.resolved_library();
                let list = ids
                    .iter()
                    .map(|id| library.get(id).cloned().context("Missing asset"))
                    .collect::<Result<Vec<_>>>()?;
                let path = rfd::FileDialog::new()
                    .set_file_name(format!("{}.mloom_fabric", render::safe_export_name(&name)))
                    .add_filter("Maploom Fabric", &["mloom_fabric"])
                    .save_file()
                    .context("Cancelled")?;
                assets::save_fabric(&path, &name, &author, &list)?;
            }
            Action::Crop {
                asset,
                name,
                rect,
                flip_x,
                flip_y,
                turns,
            } => {
                let library = self.resolved_library();
                let source = library.get(&asset).context("Missing asset")?;
                let mut a = assets::crop_asset(source, rect, flip_x, flip_y, turns)?;
                a.meta.name = name;
                let mut p = self.project.clone();
                p.assets.insert(a.meta.id.clone(), a.meta.clone());
                self.commit(p)?;
                self.library.insert(a.meta.id.clone(), a);
                return Ok(true);
            }
            Action::Autosave => {
                if self.project_open {
                    recovery::save(&recovery::root(), &self.project, &self.resolved_library())?;
                }
            }
            Action::Recover => {
                let backup = if self.project_open {
                    recovery::latest(&recovery::root(), Some(&self.project.id))?
                } else {
                    self.recovery.clone()
                }
                .context("No recovery backup available")?;
                self.confirm_replace()?;
                let loaded = recovery::load(&backup.path)?;
                self.load_catalogs()?;
                self.project_open = true;
                self.project = loaded.project;
                self.library.retain(|id, _| self.default_ids.contains(id));
                self.library.extend(loaded.library);
                self.path = None;
                self.warnings = loaded.warnings;
                self.resolve_packs();
                self.recovered = true;
                self.recovery = None;
                self.history = History::default();
                self.revision += 1;
                return Ok(true);
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod catalog_tests {
    use super::*;

    #[test]
    fn single_asset_resolution_matches_library_and_preserves_catalog_source_identity() {
        let project = Project {
            background_asset: None,
            ..Project::default()
        };
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            saved: project.clone(),
            project,
            library: Library::new(),
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        };
        for id in ["embedded", "catalog", "unchanged"] {
            let meta = AssetMeta {
                id: id.into(),
                name: id.into(),
                path: "original.png".into(),
                tags: vec!["objects".into()],
                size: Point::new(1., 1.),
                pivot: Point::new(0.5, 0.5),
                sha256: "original checksum".into(),
                color_mode: ObjectColorMode::None,
            };
            let source = assets::Source::Bytes(Arc::new(vec![1, 2, 3]));
            editor.library.insert(
                id.into(),
                assets::Asset {
                    meta: meta.clone(),
                    source: if id == "catalog" {
                        assets::Source::Catalog {
                            id: "directory".into(),
                            source: Box::new(source),
                        }
                    } else {
                        source
                    },
                },
            );
            if id != "unchanged" {
                let mut modified = meta;
                modified.name = "Edited".into();
                modified.path = "project.png".into();
                modified.sha256 = "project checksum".into();
                modified.size = Point::new(2., 3.);
                modified.color_mode = ObjectColorMode::RedMask;
                editor.project.assets.insert(id.into(), modified);
            }
        }
        let library = editor.resolved_library();
        for id in editor.library.keys() {
            let asset = editor.resolved_asset(id).unwrap();
            assert_eq!(asset.meta, library[id].meta);
            assert_eq!(asset.source.read().unwrap(), [1, 2, 3]);
        }
        let catalog = editor.resolved_asset("catalog").unwrap();
        assert_eq!(catalog.meta.name, "Edited");
        assert_eq!(catalog.meta.path, "original.png");
        assert_eq!(catalog.meta.sha256, "original checksum");
        assert_eq!(
            editor.resolved_asset("embedded").unwrap().meta.path,
            "project.png"
        );
        assert!(editor.resolved_asset("missing").is_none());
    }

    #[test]
    fn variation_actions_copy_locked_content_switch_floor_sets_and_support_undo() {
        let mut project = Project {
            background_asset: None,
            use_default_assets: false,
            ..Project::default()
        };
        project.levels[0].layers[0].locked = true;
        let main_id = project.levels[0].id.clone();
        let base = project.id.clone();
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            saved: project.clone(),
            project,
            library: Library::new(),
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        };
        editor
            .apply(Action::AddVariation {
                name: "Night".into(),
            })
            .unwrap();
        let night = editor.project.active_variation.clone().unwrap();
        let mut changed = editor.project.clone();
        changed.levels.push(Level::new("Night tower"));
        editor.commit(changed).unwrap();
        editor
            .apply(Action::SelectVariation { id: base.clone() })
            .unwrap();
        assert_eq!(editor.project.levels.len(), 1);
        assert_eq!(editor.project.levels[0].id, main_id);
        let mut invalid = editor.project.clone();
        invalid.levels = vec![Level::new("Replacement")];
        assert!(editor.commit(invalid).is_err());
        editor
            .apply(Action::SelectVariation { id: night.clone() })
            .unwrap();
        assert_eq!(editor.project.levels.len(), 2);
        editor
            .apply(Action::RenameVariation {
                name: "Dusk".into(),
            })
            .unwrap();
        let before = editor.project.clone();
        editor.apply(Action::RemoveVariation).unwrap();
        assert_eq!(editor.project.active_variation, Some(base));
        editor.apply(Action::Undo).unwrap();
        assert_eq!(editor.project, before);
        editor.apply(Action::Redo).unwrap();
        assert_eq!(editor.project.variations.len(), 1);
        assert!(editor.apply(Action::RemoveVariation).is_err());
        let mut forged = editor.project.clone();
        forged.active_variation = Some(night);
        assert!(editor.commit(forged).is_err());
    }

    #[test]
    fn welcome_state_new_resize_undo_redo_and_open_use_the_native_actions() {
        let project = Project {
            background_asset: None,
            ..Project::default()
        };
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: false,
            project: project.clone(),
            saved: project,
            library: Library::new(),
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        };
        assert!(!editor.snapshot(false).project_open);
        assert!(!editor.dirty());
        assert!(editor.apply(Action::Save { save_as: false }).is_err());
        assert!(
            editor
                .apply(Action::ResizeMap {
                    top: 1,
                    right: 0,
                    bottom: 0,
                    left: 0
                })
                .is_err()
        );
        assert!(
            editor
                .apply(Action::New {
                    name: "Bad".into(),
                    width: 0,
                    height: 10,
                    demo: false
                })
                .is_err()
        );
        assert!(!editor.project_open);
        editor
            .apply(Action::New {
                name: "Table".into(),
                width: 48,
                height: 27,
                demo: false,
            })
            .unwrap();
        assert!(editor.snapshot(false).project_open);
        assert!(!editor.dirty());
        assert_eq!((editor.project.width, editor.project.height), (48, 27));
        assert!(
            editor
                .project
                .levels
                .iter()
                .flat_map(|l| &l.layers)
                .all(|l| l.items.is_empty())
        );
        editor.project.levels[0].layers[0].locked = true;
        let mut text = Item::new(
            "Locked",
            ItemKind::Text {
                position: Point::new(4., 5.),
                text: "Test".into(),
                size: 1.,
                color: [255; 4],
            },
        );
        text.locked = true;
        editor.project.levels[0].layers[0].items.push(text);
        editor.saved = editor.project.clone();
        let original = editor.project.clone();
        editor
            .apply(Action::ResizeMap {
                top: 2,
                right: -1,
                bottom: 0,
                left: 3,
            })
            .unwrap();
        assert!(editor.dirty());
        assert_eq!((editor.project.width, editor.project.height), (50, 29));
        assert_eq!(
            editor.project.levels[0].layers[0].items[0].points(),
            [Point::new(7., 7.)]
        );
        let resized = editor.project.clone();
        editor.apply(Action::Undo).unwrap();
        assert_eq!(editor.project, original);
        assert!(!editor.dirty());
        editor.apply(Action::Redo).unwrap();
        assert_eq!(editor.project, resized);
        let revision = editor.revision;
        assert!(
            editor
                .apply(Action::ResizeMap {
                    top: -512,
                    right: 0,
                    bottom: 0,
                    left: 0
                })
                .is_err()
        );
        assert_eq!(editor.project, resized);
        assert_eq!(editor.revision, revision);
        editor
            .apply(Action::ResizeMap {
                top: 0,
                right: 0,
                bottom: 0,
                left: 0,
            })
            .unwrap();
        assert_eq!(editor.revision, revision);
        editor.apply(Action::Undo).unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("table.mloom");
        formats::save(&path, &resized, &editor.library).unwrap();
        editor.project_open = false;
        editor
            .apply(Action::Open {
                path: Some(path.to_string_lossy().into_owned()),
            })
            .unwrap();
        assert!(editor.project_open);
        assert!(!editor.dirty());
        assert_eq!(editor.project, resized);
        assert!(!editor.history.can_undo());
    }

    #[test]
    fn placing_windows_and_mixed_name_doors_exports_their_correct_types() {
        let project = Project {
            background_asset: None,
            use_default_assets: false,
            ..Project::default()
        };
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            project: project.clone(),
            saved: project,
            library: Library::new(),
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        };
        editor.project.levels[0].layers[0].items.push(Item::new(
            "Wall",
            ItemKind::Wall {
                points: vec![Point::new(0., 0.), Point::new(8., 0.)],
                width: 0.1,
                color: [255; 4],
                blocks_sight: true,
            },
        ));
        for (index, name) in ["BlueLinedLongWindow", "WindowDoor", "abc123"]
            .into_iter()
            .enumerate()
        {
            let id = format!("test:/portals/{name}.png");
            editor.library.insert(
                id.clone(),
                assets::Asset {
                    meta: AssetMeta {
                        id: id.clone(),
                        name: name.into(),
                        path: id.clone(),
                        tags: if name == "abc123" {
                            vec!["Windows".into()]
                        } else {
                            vec![]
                        },
                        size: Point::new(1., 1.),
                        pivot: Point::new(0.5, 0.5),
                        sha256: String::new(),
                        color_mode: ObjectColorMode::None,
                    },
                    source: assets::Source::Bytes(Arc::new(vec![])),
                },
            );
            editor
                .apply(Action::Portal {
                    level: 0,
                    layer: 1,
                    position: Point::new(1. + index as f32 * 2., 0.),
                    asset: id,
                    color: [255; 4],
                })
                .unwrap();
        }
        let vtt = formats::dd2vtt(&editor.project, 0, 100, b"PNG", &editor.library);
        let flags: Vec<_> = vtt["portals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["closed"].as_bool().unwrap())
            .collect();
        assert_eq!(flags, [false, true, false]);
        assert_eq!(
            editor.project.levels[0].layers[1].items[0].portal_type,
            Some(PortalType::Window)
        );
        editor.apply(Action::Undo).unwrap();
        assert_eq!(editor.project.levels[0].layers[1].items.len(), 2);
        editor.apply(Action::Redo).unwrap();
        assert_eq!(
            editor.project.levels[0].layers[1].items[2].portal_type,
            Some(PortalType::Window)
        );
    }

    #[test]
    fn catalog_selection_undo_redo_new_and_open_rebuild_the_active_library() {
        let temp = tempfile::tempdir().unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba([120, 80, 40, 255]))
            .save(temp.path().join("item.png"))
            .unwrap();
        let directory = catalog::directory(temp.path(), None).unwrap();
        let project = Project {
            background_asset: None,
            ..Project::default()
        };
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            project: project.clone(),
            saved: project,
            library: Library::new(),
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![catalog::scan(&directory)],
        };
        let mut selected = editor.project.clone();
        selected.asset_catalogs.push(CatalogRef {
            id: directory.id,
            name: directory.name,
            packs: Some(vec![String::new()]),
        });
        assert!(
            editor
                .apply(Action::Commit {
                    project: Box::new(selected)
                })
                .unwrap()
        );
        assert_eq!(editor.library.len(), 1);
        assert!(editor.snapshot(false).catalogs.is_none());
        assert!(editor.apply(Action::Undo).unwrap());
        assert!(editor.library.is_empty());
        assert!(editor.apply(Action::Redo).unwrap());
        assert_eq!(editor.library.len(), 1);

        editor.saved = editor.project.clone();
        assert!(
            editor
                .apply(Action::New {
                    name: "Other".into(),
                    width: 8,
                    height: 8,
                    demo: false
                })
                .unwrap()
        );
        assert!(editor.project.asset_catalogs.is_empty());
        assert!(editor.library.is_empty());

        let path = temp.path().join("empty.mloom");
        formats::save(&path, &editor.project, &editor.library).unwrap();
        let mut selected = editor.project.clone();
        selected.asset_catalogs.push(CatalogRef {
            id: editor.catalogs[0].id.clone(),
            name: "Assets".into(),
            packs: None,
        });
        editor.commit(selected).unwrap();
        assert_eq!(editor.library.len(), 1);
        editor.saved = editor.project.clone();
        assert!(
            editor
                .apply(Action::Open {
                    path: Some(path.to_string_lossy().into_owned())
                })
                .unwrap()
        );
        assert!(editor.library.is_empty());
        assert!(editor.project.asset_catalogs.is_empty());
        assert!(!editor.dirty());
    }
}
#[tauri::command]
async fn editor_snapshot(
    app: tauri::AppHandle,
    startup: tauri::State<'_, EditorStartup>,
) -> std::result::Result<Snapshot, String> {
    let startup = startup.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = startup.initialize()?;
        // Concurrent/repeated snapshot requests share exactly one editor.
        app.manage(state.clone());
        let editor = state.0.lock().map_err(|e| e.to_string())?;
        Ok(editor.snapshot(true))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn editor_action(
    state: tauri::State<'_, DesktopState>,
    action: Action,
    revision: u64,
) -> std::result::Result<Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Snapshot> {
        let mut e = state.0.lock().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        ensure!(
            revision == e.revision,
            "Document changed; retry after refresh"
        );
        let before_assets = e.project.assets.clone();
        let library = e.apply(action)? || before_assets != e.project.assets;
        Ok(e.snapshot(library))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}
#[tauri::command]
async fn editor_scene(
    state: tauri::State<'_, DesktopState>,
    level: usize,
    grid: bool,
    lights: bool,
    known_textures: Option<Vec<String>>,
    known_meshes: Option<Vec<String>>,
) -> std::result::Result<GpuScene, String> {
    let (project, library, revision) = {
        let e = state.0.lock().map_err(|e| e.to_string())?;
        (e.project.clone(), e.resolved_library(), e.revision)
    };
    let renderer = state.1.clone();
    let known: HashSet<_> = known_textures.unwrap_or_default().into_iter().collect();
    tauri::async_runtime::spawn_blocking(move || {
        renderer
            .lock()
            .map_err(|e| e.to_string())?
            .scene(
                &project,
                &library,
                SceneOptions {
                    level,
                    grid,
                    lights,
                    revision,
                    known_meshes: known_meshes.unwrap_or_default().into_iter().collect(),
                },
                &known,
            )
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[derive(Deserialize)]
struct DrawingPreviewRequest {
    level: usize,
    layer: usize,
    item: Item,
    under: bool,
    grid: bool,
    lights: bool,
    revision: u64,
}

fn drawing_preview_project(project: &Project, request: &DrawingPreviewRequest) -> Result<Project> {
    let mut preview = project.clone();
    let layer = preview
        .levels
        .get_mut(request.level)
        .and_then(|l| l.layers.get_mut(request.layer))
        .context("Invalid layer")?;
    ensure!(layer.visible && !layer.locked, "Layer is not editable");
    // Use the commit path for rooms too: overlaps must preview their merged
    // floors and walls, rather than rendering an unrelated polygon on top.
    if matches!(request.item.kind, ItemKind::Room { .. }) {
        editing::add_room(
            &mut preview,
            request.level,
            request.layer,
            request.item.clone(),
        )?;
    } else if request.under {
        layer.items.insert(0, request.item.clone());
    } else {
        layer.items.push(request.item.clone());
    }
    editing::protect_locked(project, &preview)?;
    editing::reconcile_geometry(project, &mut preview);
    preview.sync_portals();
    preview.validate()?;
    Ok(preview)
}

#[cfg(test)]
mod drawing_preview_tests {
    use super::*;

    #[test]
    fn recovery_opens_an_unsaved_document_and_keeps_the_archive_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project {
            name: "Recovery test".into(),
            background_asset: None,
            ..Project::default()
        };
        let path = recovery::save(dir.path(), &project, &Library::new()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let mut editor = editor(Project::default(), Library::new());
        editor.project_open = false;
        editor.recovery = recovery::latest(dir.path(), None).unwrap();
        assert!(editor.snapshot(false).recovery.is_some());
        assert!(editor.apply(Action::Recover).unwrap());
        assert_eq!(editor.project, project);
        assert!(editor.project_open);
        assert!(editor.dirty());
        assert!(editor.path.is_none());
        assert!(editor.snapshot(false).recovery.is_none());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }

    fn request(item: Item) -> DrawingPreviewRequest {
        DrawingPreviewRequest {
            level: 0,
            layer: 1,
            item,
            under: false,
            grid: false,
            lights: false,
            revision: 0,
        }
    }

    fn editor(project: Project, library: Library) -> Editor {
        Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            saved: project.clone(),
            project,
            library,
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        }
    }

    #[test]
    fn drawing_preview_is_transient_and_uses_commit_geometry() {
        let project = Project {
            background_asset: None,
            ..Project::default()
        };
        let before = serde_json::to_value(&project).unwrap();
        let mut library = Library::new();
        let mut bytes = Cursor::new(vec![]);
        image::RgbaImage::from_pixel(16, 16, image::Rgba([150, 100, 60, 255]))
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        let id = "test:/drawing.png";
        let meta = AssetMeta {
            id: id.into(),
            name: "Drawing material".into(),
            path: id.into(),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: ObjectColorMode::None,
        };
        library.insert(
            id.into(),
            assets::Asset {
                meta,
                source: assets::Source::Bytes(Arc::new(bytes)),
            },
        );
        let mut editor = editor(project.clone(), library);
        let points = vec![
            Point::new(2., 2.),
            Point::new(6., 2.),
            Point::new(6., 6.),
            Point::new(2., 6.),
        ];
        let mut kinds = vec![
            ItemKind::Wall {
                points: points.clone(),
                width: 0.4,
                color: [255; 4],
                blocks_sight: true,
            },
            ItemKind::Floor {
                points: points.clone(),
                color: [120, 80, 60, 255],
            },
            ItemKind::Room {
                rings: vec![points.clone()],
                floor: Some(id.into()),
                wall: Some(id.into()),
                width: 0.3,
            },
            ItemKind::Path {
                points: points.clone(),
                width: 0.5,
                color: [255; 4],
            },
        ];
        for roof_type in [
            crate::roofs::RoofType::Gable,
            crate::roofs::RoofType::Hip,
            crate::roofs::RoofType::Dormer,
        ] {
            kinds.push(ItemKind::Roof {
                points: vec![Point::new(2., 4.), Point::new(8., 4.)],
                width: 3.,
                roof_type,
                ridge: Some(id.into()),
                edge: Some(id.into()),
                hip: Some(id.into()),
                shade: true,
                sun_direction: 135.,
                contrast: 0.3,
            });
        }
        for kind in kinds {
            let history = (editor.history.can_undo(), editor.history.can_redo());
            let req = request(Item::new("Drawing", kind).with_texture(Some(id)));
            let preview = drawing_preview_project(&project, &req).unwrap();
            assert_eq!(serde_json::to_value(&project).unwrap(), before);
            assert_eq!(
                preview.levels[0].layers[1].items.last().unwrap().kind,
                req.item.kind
            );
            let options = || SceneOptions {
                level: 0,
                grid: false,
                lights: false,
                revision: 0,
                known_meshes: HashSet::new(),
            };
            let mut renderer = SceneRenderer::default();
            let base = renderer
                .scene(&project, &editor.library, options(), &HashSet::new())
                .unwrap();
            let scene = renderer
                .scene(&preview, &editor.library, options(), &HashSet::new())
                .unwrap();
            assert_ne!(
                serde_json::to_value(&base.meshes).unwrap(),
                serde_json::to_value(&scene.meshes).unwrap()
            );
            assert_eq!(editor.project, project);
            assert!(!editor.dirty());
            assert_eq!(
                (editor.history.can_undo(), editor.history.can_redo()),
                history
            );
            editor
                .apply(Action::Add {
                    level: 0,
                    layer: 1,
                    item: Box::new(req.item),
                })
                .unwrap();
            assert_eq!(editor.project.levels, preview.levels);
            editor.apply(Action::Undo).unwrap();
            assert_eq!(editor.project, project);
        }
    }

    #[test]
    fn drawing_preview_merges_overlapping_rooms_like_commit() {
        let mut project = Project {
            background_asset: None,
            ..Project::default()
        };
        let room = |x| {
            Item::new(
                "Room",
                ItemKind::Room {
                    rings: vec![vec![
                        Point::new(x, 2.),
                        Point::new(x + 4., 2.),
                        Point::new(x + 4., 6.),
                        Point::new(x, 6.),
                    ]],
                    floor: None,
                    wall: None,
                    width: 0.3,
                },
            )
        };
        project.levels[0].layers[1].items.push(room(2.));
        let req = request(room(4.));
        let preview = drawing_preview_project(&project, &req).unwrap();
        assert_eq!(preview.levels[0].layers[1].items.len(), 1);
        assert_eq!(project.levels[0].layers[1].items.len(), 1);
        let mut editor = editor(project, Library::new());
        editor
            .apply(Action::Add {
                level: 0,
                layer: 1,
                item: Box::new(req.item),
            })
            .unwrap();
        assert_eq!(editor.project.levels, preview.levels);
    }

    #[test]
    fn drawing_preview_respects_layer_order_and_editability() {
        let mut project = Project::default();
        let mut req = request(Item::new(
            "Drawing",
            ItemKind::Wall {
                points: vec![Point::new(1., 1.), Point::new(5., 1.)],
                width: 0.3,
                color: [255; 4],
                blocks_sight: true,
            },
        ));
        project.levels[0].layers[1]
            .items
            .push(Item::new("Existing", req.item.kind.clone()));
        req.under = true;
        let preview = drawing_preview_project(&project, &req).unwrap();
        assert_eq!(preview.levels[0].layers[1].items[0].id, req.item.id);
        project.levels[0].layers[1].locked = true;
        assert!(drawing_preview_project(&project, &req).is_err());
        project.levels[0].layers[1].locked = false;
        project.levels[0].layers[1].visible = false;
        assert!(drawing_preview_project(&project, &req).is_err());
        req.layer = usize::MAX;
        assert!(drawing_preview_project(&project, &req).is_err());
    }
}

#[tauri::command]
async fn editor_drawing_preview(
    state: tauri::State<'_, DesktopState>,
    request: DrawingPreviewRequest,
    known_textures: Option<Vec<String>>,
    known_meshes: Option<Vec<String>>,
) -> std::result::Result<GpuScene, String> {
    let (project, library) = {
        let e = state.0.lock().map_err(|e| e.to_string())?;
        if e.revision != request.revision {
            return Err("Placement changed".into());
        }
        (e.project.clone(), e.resolved_library())
    };
    let renderer = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GpuScene> {
        let project = drawing_preview_project(&project, &request)?;
        renderer
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .scene(
                &project,
                &library,
                SceneOptions {
                    level: request.level,
                    grid: request.grid,
                    lights: request.lights,
                    revision: request.revision,
                    known_meshes: known_meshes.unwrap_or_default().into_iter().collect(),
                },
                &known_textures.unwrap_or_default().into_iter().collect(),
            )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[derive(Deserialize)]
struct PortalPreviewRequest {
    level: usize,
    layer: usize,
    position: Point,
    asset: String,
    color: Color,
    grid: bool,
    lights: bool,
    revision: u64,
}

fn portal_preview_project(
    project: &Project,
    library: &Library,
    request: &PortalPreviewRequest,
) -> Result<Project> {
    let meta = &library
        .get(&request.asset)
        .context("Missing door asset")?
        .meta;
    let item = editing::portal_item(
        project,
        request.level,
        request.position,
        meta,
        request.color,
    )?;
    let mut preview = project.clone();
    let layer = preview
        .levels
        .get_mut(request.level)
        .and_then(|l| l.layers.get_mut(request.layer))
        .context("Invalid layer")?;
    ensure!(layer.visible && !layer.locked, "Layer is not editable");
    layer.items.push(item);
    Ok(preview)
}

#[cfg(test)]
mod portal_preview_tests {
    use super::*;

    #[test]
    fn preview_matches_installation_without_editing_document_or_history() {
        let mut library = Library::new();
        for (id, color, size) in [
            (
                "test:/walls/stone.png",
                [80, 85, 90, 255],
                Point::new(1., 0.3),
            ),
            (
                "test:/tilesets/floor.png",
                [156, 113, 68, 255],
                Point::new(1., 1.),
            ),
            (
                "test:/portals/door.png",
                [40, 180, 230, 255],
                Point::new(1.5, 0.3),
            ),
            (
                "test:/portals/window.png",
                [170, 220, 250, 255],
                Point::new(1.2, 0.3),
            ),
        ] {
            let mut bytes = Cursor::new(vec![]);
            image::RgbaImage::from_pixel(60, 12, image::Rgba(color))
                .write_to(&mut bytes, image::ImageFormat::Png)
                .unwrap();
            let bytes = bytes.into_inner();
            let meta = AssetMeta {
                id: id.into(),
                name: id.rsplit('/').next().unwrap().into(),
                path: id.into(),
                tags: vec![],
                size,
                pivot: Point::new(0.5, 0.5),
                sha256: assets::hash(&bytes),
                color_mode: ObjectColorMode::None,
            };
            library.insert(
                id.into(),
                assets::Asset {
                    meta,
                    source: assets::Source::Bytes(Arc::new(bytes)),
                },
            );
        }
        let mut project = Project {
            name: "Curved portal preview".into(),
            width: 20,
            height: 20,
            background_asset: None,
            use_default_assets: false,
            ..Project::default()
        };
        let ring = (0..512)
            .map(|n| {
                let a = n as f32 * std::f32::consts::TAU / 512.;
                Point::new(10. + 5. * a.cos(), 10. + 5. * a.sin())
            })
            .collect();
        project.levels[0].layers[1].items.push(Item::new(
            "Round room",
            ItemKind::Room {
                rings: vec![ring],
                floor: Some("test:/tilesets/floor.png".into()),
                wall: Some("test:/walls/stone.png".into()),
                width: 0.3,
            },
        ));
        let mut editor = Editor {
            resources_loaded: true,
            recovered: false,
            recovery: None,
            project_open: true,
            saved: project.clone(),
            project: project.clone(),
            library,
            default_ids: HashSet::new(),
            prefabs: Store::default(),
            history: History::default(),
            revision: 0,
            path: None,
            warnings: vec![],
            catalog_warnings: vec![],
            preferences: Preferences::default(),
            locales: Locales::default(),
            catalogs: vec![],
        };
        let make_scene = |p: &Project| {
            SceneRenderer::default()
                .scene(
                    p,
                    &editor.library,
                    SceneOptions {
                        level: 0,
                        grid: false,
                        lights: false,
                        revision: 0,
                        known_meshes: HashSet::new(),
                    },
                    &HashSet::new(),
                )
                .unwrap()
        };
        let base = make_scene(&project);
        let mut cases = vec![];
        for asset in ["test:/portals/door.png", "test:/portals/window.png"] {
            for position in [
                Point::new(15., 10.),
                Point::new(14.6, 12.1),
                Point::new(10., 15.),
            ] {
                let request = PortalPreviewRequest {
                    level: 0,
                    layer: 1,
                    position,
                    asset: asset.into(),
                    color: [255; 4],
                    grid: false,
                    lights: false,
                    revision: 0,
                };
                let preview =
                    portal_preview_project(&editor.project, &editor.library, &request).unwrap();
                let item = preview.levels[0].layers[1].items.last().unwrap();
                let scene = make_scene(&preview);
                assert_ne!(
                    serde_json::to_value(&base.meshes).unwrap(),
                    serde_json::to_value(&scene.meshes).unwrap()
                );
                assert_eq!(editor.project, project);
                assert!(
                    !editor.dirty() && !editor.history.can_undo() && !editor.history.can_redo()
                );
                let expected = editing::portal_item(
                    &project,
                    0,
                    position,
                    &editor.library.get(asset).unwrap().meta,
                    [255; 4],
                )
                .unwrap();
                assert_eq!(item.kind, expected.kind);
                assert_eq!(item.portal_type, expected.portal_type);
                assert_eq!(item.wall_attachment, expected.wall_attachment);
                cases.push(serde_json::json!({"asset": asset, "position": position, "scene": scene, "item": item}));
            }
        }
        let invalid = PortalPreviewRequest {
            level: 0,
            layer: 1,
            position: Point::new(10., 10.),
            asset: "test:/portals/door.png".into(),
            color: [255; 4],
            grid: false,
            lights: false,
            revision: 0,
        };
        assert!(portal_preview_project(&editor.project, &editor.library, &invalid).is_err());
        let locked = PortalPreviewRequest {
            position: Point::new(15., 10.),
            ..invalid
        };
        editor.project.levels[0].layers[1].locked = true;
        assert!(portal_preview_project(&editor.project, &editor.library, &locked).is_err());
        editor.project.levels[0].layers[1].locked = false;
        if let Some(directory) = std::env::var_os("MAPLOOM_PORTAL_UI_FIXTURE") {
            let directory = PathBuf::from(directory);
            std::fs::create_dir_all(&directory).unwrap();
            let thumbnails: std::collections::BTreeMap<_, _> = editor
                .library
                .iter()
                .map(|(id, asset)| {
                    let assets::Source::Bytes(bytes) = &asset.source else {
                        panic!()
                    };
                    (
                        id,
                        format!(
                            "data:image/png;base64,{}",
                            STANDARD.encode(bytes.as_slice())
                        ),
                    )
                })
                .collect();
            std::fs::write(directory.join("portal-preview.json"), serde_json::to_vec(&serde_json::json!({
                "snapshot": editor.snapshot(true), "base": base, "cases": cases, "thumbnails": thumbnails,
            })).unwrap()).unwrap();
        }
        editor
            .apply(Action::Portal {
                level: 0,
                layer: 1,
                position: Point::new(15., 10.),
                asset: "test:/portals/door.png".into(),
                color: [255; 4],
            })
            .unwrap();
        let installed = &editor.project.levels[0].layers[1].items[1];
        let expected: Item = serde_json::from_value(cases[0]["item"].clone()).unwrap();
        assert_eq!(installed.kind, expected.kind);
        assert_eq!(installed.wall_attachment, expected.wall_attachment);
        assert!(editor.dirty() && editor.history.can_undo());
        editor.apply(Action::Undo).unwrap();
        assert_eq!(editor.project, project);
        editor.apply(Action::Redo).unwrap();
        assert_eq!(editor.project.levels[0].layers[1].items.len(), 2);
    }
}

#[tauri::command]
async fn editor_portal_preview(
    state: tauri::State<'_, DesktopState>,
    request: PortalPreviewRequest,
    known_textures: Option<Vec<String>>,
    known_meshes: Option<Vec<String>>,
) -> std::result::Result<GpuScene, String> {
    // Preview is a transient copy: it cannot alter history, dirty state or the
    // authoritative project. The same placement and renderer are used on click.
    let (project, library) = {
        let e = state.0.lock().map_err(|e| e.to_string())?;
        if e.revision != request.revision {
            return Err("Placement changed".into());
        }
        (e.project.clone(), e.resolved_library())
    };
    let renderer = state.1.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<GpuScene> {
        let project = portal_preview_project(&project, &library, &request)?;
        renderer
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .scene(
                &project,
                &library,
                SceneOptions {
                    level: request.level,
                    grid: request.grid,
                    lights: request.lights,
                    revision: request.revision,
                    known_meshes: known_meshes.unwrap_or_default().into_iter().collect(),
                },
                &known_textures.unwrap_or_default().into_iter().collect(),
            )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn asset_silhouette(
    state: tauri::State<'_, DesktopState>,
    id: String,
) -> std::result::Result<Vec<Point>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<Point>> {
        let asset = state
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .resolved_asset(&id)
            .context("Missing asset")?;
        Ok(assets::alpha_silhouette(&assets::decode_surface(&asset)?))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn asset_thumbnail(
    state: tauri::State<'_, DesktopState>,
    id: String,
    color: Option<Color>,
    max_side: Option<u32>,
) -> std::result::Result<String, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String> {
        let asset = state
            .0
            .lock()
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .resolved_asset(&id)
            .context("Missing asset")?;
        let mut image = assets::decode_surface(&asset)?;
        if let Some(color) = color {
            let mode = if assets::category(&asset.meta) == "objects" {
                asset.meta.object_color_mode()
            } else {
                ObjectColorMode::Tint
            };
            match mode {
                ObjectColorMode::RedMask => assets::recolor_red_mask(&mut image, color),
                ObjectColorMode::Tint => {
                    for pixel in image.pixels_mut() {
                        pixel.0 = multiply_colors(pixel.0, color);
                    }
                }
                ObjectColorMode::None => {}
            }
        }
        // Fit every asset into the preview bounds without changing its proportions.
        let side = max_side.unwrap_or(160).clamp(16, 2048);
        let small = image::DynamicImage::ImageRgba8(image).thumbnail(side, side);
        let mut bytes = Cursor::new(vec![]);
        small.write_to(&mut bytes, image::ImageFormat::Png)?;
        Ok(format!(
            "data:image/png;base64,{}",
            STANDARD.encode(bytes.into_inner())
        ))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn editor_fonts() -> std::result::Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(crate::text::local_fonts)
        .await
        .map_err(|e| e.to_string())
}

pub fn run(demo: bool, path: Option<PathBuf>) -> Result<()> {
    let close_allowed = Arc::new(AtomicBool::new(false));
    // Read only the small settings file before showing the window; the first
    // snapshot initializes the editor, and libraries load only when needed.
    let preferences = Preferences::load();
    let startup_theme = preferences.theme.as_ref().and_then(|id| {
        crate::platform::themes::read_variant(
            &crate::platform::themes::directory().ok()?,
            id,
            preferences.theme_variant.as_deref(),
        )
        .ok()
    });
    let dark = startup_theme
        .as_ref()
        .and_then(|theme| theme.manifest.mode.as_ref())
        .map_or(preferences.dark, |mode| {
            *mode == crate::platform::themes::ThemeMode::Dark
        });
    let startup_theme_json = serde_json::to_string(&startup_theme)?;
    let shell_key = uuid::Uuid::new_v4().to_string();
    let mut context = tauri::generate_context!();
    for window in &mut context.config_mut().app.windows {
        if let Some(branding) = startup_theme
            .as_ref()
            .and_then(|theme| theme.manifest.branding.as_ref())
        {
            window.title = branding.name.clone();
        }
        window.background_color = Some(if dark {
            tauri::window::Color(16, 18, 24, 255)
        } else {
            tauri::window::Color(230, 232, 235, 255)
        });
    }
    tauri::Builder::default()
        .register_uri_scheme_protocol("maploom-theme", |_context, request| themes::resource(request))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::UpdateState::default())
        .append_invoke_initialization_script(format!(
            "if (window === window.top) {{ window.__MAPLOOM_DARK__ = {dark}; window.__MAPLOOM_THEME__ = {startup_theme_json}; Object.defineProperty(window, '__MAPLOOM_INVOKE__', {{value: (command, payload) => window.__TAURI_INTERNALS__.invoke(command, payload, {{headers: {{'x-maploom-shell': '{shell_key}'}}}})}}); }}"
        ))
        .setup(move |app| {
            crate::resources::set_bundle_directory(app.path().resource_dir()?);
            app.manage(EditorStartup::new(demo, path));
            Ok(())
        })
        .invoke_handler({
            let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![editor_snapshot,editor_action,editor_scene,editor_portal_preview,editor_drawing_preview,asset_thumbnail,asset_silhouette,editor_fonts,themes::theme_catalog,themes::theme_window_title,themes::theme_package,themes::theme_preview,themes::theme_open_directory,themes::theme_create_example,updates::update_check,updates::update_install,updates::update_open_release];
            move |invoke| {
                if invoke.message.headers().get("x-maploom-shell").and_then(|value| value.to_str().ok()) != Some(shell_key.as_str()) {
                    invoke.resolver.reject("Editor commands are available only to the application shell");
                    return true;
                }
                handler(invoke)
            }
        })
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<updates::UpdateState>().installing.load(Ordering::Acquire) {
                    api.prevent_close();
                    return;
                }
                if close_allowed.load(Ordering::Relaxed) { return; }
                // A loading window can be closed before an editor exists.
                let Some(state) = window.try_state::<DesktopState>() else { return; };
                let dirty = state.0.try_lock().map_or(true, |e| e.dirty());
                if dirty {
                    api.prevent_close();
                    let window = window.clone();
                    let allowed = close_allowed.clone();
                    tauri::async_runtime::spawn(async move {
                        let result = rfd::AsyncMessageDialog::new().set_title("Maploom").set_description("Несохранённые изменения / Unsaved changes\nЗакрыть без сохранения? / Close without saving?").set_buttons(rfd::MessageButtons::YesNo).show().await;
                        if result == rfd::MessageDialogResult::Yes {
                            allowed.store(true, Ordering::Relaxed);
                            let _ = window.close();
                        }
                    });
                }
            }
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event { let _ = window.emit("maploom-drop", paths); }
        })
        .run(context).map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
