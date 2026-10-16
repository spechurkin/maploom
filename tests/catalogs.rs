use maploom::{
    assets::{self, Library, Source},
    catalog,
    document::{CatalogRef, Project},
};
use std::{io::Write, path::Path};

fn png(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbaImage::from_pixel(8, 4, image::Rgba([120, 40, 20, 255]))
        .save(path)
        .unwrap();
}
fn selected(directory: &maploom::preferences::AssetDirectory, asset: &str) -> Project {
    Project {
        background_asset: Some(asset.into()),
        asset_catalogs: vec![CatalogRef {
            id: directory.id.clone(),
            name: directory.name.clone(),
            packs: None,
        }],
        ..Project::default()
    }
}
#[test]
fn recursive_catalogs_are_project_scoped_and_external_projects_survive_relocation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("original");
    png(&root.join("textures/terrain/ground.png"));
    png(&root.join("nested/objects/chair.png"));
    std::fs::write(root.join("ignored.txt"), b"not an image").unwrap();
    let directory = catalog::directory(&root, None).unwrap();
    let indexed = catalog::scan(&directory);
    assert!(indexed.warnings.is_empty(), "{:?}", indexed.warnings);
    assert_eq!(indexed.library.len(), 2);
    let id = "res://textures/terrain/ground.png";
    let mut project = selected(&directory, id);
    let mut library = Library::new();
    assert!(
        catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed)).is_empty()
    );
    let expected = assets::verified_bytes(&library[id]).unwrap();
    for extension in ["mloom", "mloom.json"] {
        let path = temp.path().join(format!("project.{extension}"));
        maploom::formats::save(&path, &project, &library).unwrap();
        let mut loaded = maploom::formats::load(&path).unwrap();
        assert!(loaded.library.is_empty());
        assert_eq!(loaded.project.external_assets[id], directory.id);
        assert!(!serde_json::to_string(&loaded.project)
            .unwrap()
            .contains(&directory.path.replace('\\', "\\\\")));
        let mut archive = if extension == "mloom" {
            Some(zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap())
        } else {
            None
        };
        if let Some(zip) = &mut archive {
            assert_eq!(zip.len(), 1);
        }
        drop(archive);
        // Missing catalogs preserve geometry and allow a subsequent save.
        assert!(
            catalog::resolve(&mut loaded.project, &mut loaded.library, &[])
                .iter()
                .any(|w| w.contains("Missing asset catalog"))
        );
        maploom::formats::save(&path, &loaded.project, &loaded.library).unwrap();
        assert!(catalog::resolve(
            &mut loaded.project,
            &mut loaded.library,
            std::slice::from_ref(&indexed)
        )
            .is_empty());
        assert_eq!(
            assets::verified_bytes(&loaded.library[id]).unwrap(),
            expected
        );
    }
    // A different project must never see the preceding project's catalogs.
    let mut other = Project {
        background_asset: None,
        ..Project::default()
    };
    catalog::resolve(&mut other, &mut library, std::slice::from_ref(&indexed));
    assert!(library.is_empty());
    // Directory relinking keeps asset IDs and resolves existing saved references.
    let moved = temp.path().join("moved");
    std::fs::rename(&root, &moved).unwrap();
    let relocated = catalog::directory(&moved, Some(directory.id.clone())).unwrap();
    let moved_catalog = catalog::scan(&relocated);
    assert_eq!(
        moved_catalog.library.keys().collect::<Vec<_>>(),
        indexed.library.keys().collect::<Vec<_>>()
    );
    assert!(catalog::resolve(&mut project, &mut library, &[moved_catalog]).is_empty());
    assert_eq!(assets::verified_bytes(&library[id]).unwrap(), expected);
    image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 80, 170, 255]))
        .save(moved.join("textures/terrain/ground.png"))
        .unwrap();
    let refreshed = catalog::scan(&relocated);
    assert!(catalog::resolve(&mut project, &mut library, &[refreshed]).is_empty());
    assert_ne!(assets::verified_bytes(&library[id]).unwrap(), expected);
    assert_eq!(project.assets[id].sha256, library[id].meta.sha256);
}

#[test]
fn scan_is_resilient_and_refresh_finds_added_files() {
    let temp = tempfile::tempdir().unwrap();
    png(&temp.path().join("nested/item.png"));
    std::fs::write(temp.path().join("broken.dungeondraft_pack"), b"broken").unwrap();
    let directory = catalog::directory(temp.path(), None).unwrap();
    let first = catalog::scan(&directory);
    assert_eq!(first.library.len(), 1);
    assert_eq!(first.warnings.len(), 1);
    png(&temp.path().join("nested/deep/second.png"));
    assert_eq!(catalog::scan(&directory).library.len(), 2);
}

#[test]
fn fabric_metadata_is_read_and_only_selected_catalogs_supply_assets() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.png");
    png(&source);
    let asset = assets::import_image(&source).unwrap();
    let packs = temp.path().join("packs/nested");
    std::fs::create_dir_all(&packs).unwrap();
    assets::save_fabric(
        &packs.join("test.mloom_fabric"),
        "Fabric title",
        "Fabric author",
        std::slice::from_ref(&asset),
    )
        .unwrap();
    let directory = catalog::directory(&temp.path().join("packs"), None).unwrap();
    let indexed = catalog::scan(&directory);
    assert_eq!(indexed.packs.len(), 1);
    assert_eq!(indexed.packs[0].name, "Fabric title");
    assert_eq!(indexed.packs[0].metadata.author, "Fabric author");
    let mut p = Project {
        background_asset: None,
        ..Project::default()
    };
    let mut library = Library::new();
    catalog::resolve(&mut p, &mut library, std::slice::from_ref(&indexed));
    assert!(library.is_empty());
    p = selected(&directory, &asset.meta.id);
    catalog::resolve(&mut p, &mut library, &[indexed]);
    assert!(matches!(
        library[&asset.meta.id].source,
        Source::Catalog { .. }
    ));
}

fn pck(path: &Path, entries: &[(&str, Vec<u8>)]) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"GDPC").unwrap();
    for number in [1u32, 3, 4, 2] {
        file.write_all(&number.to_le_bytes()).unwrap();
    }
    file.write_all(&[0; 64]).unwrap();
    file.write_all(&(entries.len() as u32).to_le_bytes())
        .unwrap();
    let mut offset = 88
        + entries
        .iter()
        .map(|(name, _)| 4 + name.len() + 32)
        .sum::<usize>() as u64;
    for (name, bytes) in entries {
        file.write_all(&(name.len() as u32).to_le_bytes()).unwrap();
        file.write_all(name.as_bytes()).unwrap();
        file.write_all(&offset.to_le_bytes()).unwrap();
        file.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
        file.write_all(&[0; 16]).unwrap();
        offset += bytes.len() as u64;
    }
    for (_, bytes) in entries {
        file.write_all(bytes).unwrap();
    }
}

#[test]
fn dd_metadata_logo_and_manifest_select_the_right_catalog_without_a_path_in_the_map() {
    let temp = tempfile::tempdir().unwrap();
    let image = temp.path().join("logo.png");
    png(&image);
    let root = temp.path().join("packs");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("named.dungeondraft_pack");
    pck(&path, &[
        ("res://packs/abc/pack.json", br#"{"id":"abc","name":"Dungeon","author":"Artist","version":2,"description":"Stone rooms","website":"https://example.org",}"#.to_vec()),
        ("res://packs/abc/preview.png", std::fs::read(&image).unwrap()),
        ("res://packs/abc/textures/objects/item.png", std::fs::read(&image).unwrap()),
    ]);
    let directory = catalog::directory(&root, None).unwrap();
    let indexed = catalog::scan(&directory);
    let pack = &indexed.packs[0];
    assert!(indexed.warnings.is_empty(), "{:?}", indexed.warnings);
    assert_eq!(pack.name, "Dungeon");
    assert_eq!(pack.metadata.author, "Artist");
    assert_eq!(pack.metadata.version, "2");
    assert_eq!(pack.metadata.description, "Stone rooms");
    assert!(pack
        .metadata
        .logo
        .as_ref()
        .unwrap()
        .starts_with("data:image/png;base64,"));
    assert_eq!(indexed.library.len(), 1, "Logo is not a map asset");
    let raw = serde_json::json!({"header":{"asset_manifest":[{"id":"abc"}]},"world":{"width":8,"height":8,"levels":{"0":{}}}});
    let mut loaded =
        maploom::formats::import_dungeondraft(raw, Path::new("without-path.dungeondraft_map"))
            .unwrap();
    catalog::select_for_dd(
        &mut loaded.project,
        std::slice::from_ref(&directory),
        std::slice::from_ref(&indexed),
    );
    assert_eq!(loaded.project.asset_catalogs[0].id, directory.id);
    assert_eq!(
        loaded.project.asset_catalogs[0].packs,
        Some(vec!["named.dungeondraft_pack".into()])
    );
    assert!(!serde_json::to_string(&loaded.project)
        .unwrap()
        .contains(&directory.path));
}

#[test]
fn old_preferences_deserialize_without_catalogs() {
    let preferences: maploom::preferences::Preferences =
        serde_json::from_str(r#"{"version":1,"locale":"ru"}"#).unwrap();
    assert!(preferences.asset_directories.is_empty());
    let old: CatalogRef = serde_json::from_str(r#"{"id":"old","name":"Packs"}"#).unwrap();
    assert!(old.packs.is_none());
}

#[test]
fn pack_selection_resolves_the_selected_copy_and_survives_save_relocation_and_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("packs");
    std::fs::create_dir_all(root.join("nested")).unwrap();
    let source = temp.path().join("source.png");
    png(&source);
    let mut first = assets::import_image(&source).unwrap();
    first.meta.id = "shared-resource".into();
    assets::save_fabric(&root.join("a.mloom_fabric"), "First", "", &[first.clone()]).unwrap();
    image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 80, 170, 255]))
        .save(&source)
        .unwrap();
    let mut second = assets::import_image(&source).unwrap();
    second.meta.id = first.meta.id.clone();
    let second_bytes = assets::verified_bytes(&second).unwrap();
    let mut unique = second.clone();
    unique.meta.id = "second-only".into();
    assets::save_fabric(
        &root.join("nested/b.mloom_fabric"),
        "Second",
        "",
        &[second, unique],
    )
        .unwrap();
    png(&root.join("loose.png"));
    let dir = catalog::directory(&root, None).unwrap();
    let indexed = catalog::scan(&dir);
    assert!(indexed
        .warnings
        .iter()
        .any(|w| w.contains("Duplicate asset")));
    assert_eq!(indexed.pack_key(&indexed.packs[1]), "nested/b.mloom_fabric");
    let mut project = selected(&dir, &first.meta.id);
    project.asset_catalogs[0].packs = Some(vec!["nested/b.mloom_fabric".into()]);
    let mut library = Library::new();
    assert!(
        catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed)).is_empty()
    );
    assert_eq!(
        library.len(),
        2,
        "Unselected packs and loose images stay out"
    );
    assert_eq!(
        assets::verified_bytes(&library[&first.meta.id]).unwrap(),
        second_bytes
    );
    project.asset_catalogs[0]
        .packs
        .as_mut()
        .unwrap()
        .push("a.mloom_fabric".into());
    assert!(
        catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed))
            .iter()
            .any(|w| w.contains("Duplicate asset"))
    );
    project.asset_catalogs[0].packs = Some(vec!["nested/b.mloom_fabric".into()]);
    catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed));
    for ext in ["mloom", "mloom.json"] {
        let path = temp.path().join(format!("map.{ext}"));
        maploom::formats::save(&path, &project, &library).unwrap();
        let loaded = maploom::formats::load(&path).unwrap();
        assert_eq!(loaded.project.asset_catalogs, project.asset_catalogs);
    }
    let moved = temp.path().join("moved");
    std::fs::rename(&root, &moved).unwrap();
    let relocated = catalog::directory(&moved, Some(dir.id.clone())).unwrap();
    let refreshed = catalog::scan(&relocated);
    assert!(catalog::resolve(&mut project, &mut library, &[refreshed]).is_empty());
    assert_eq!(
        assets::verified_bytes(&library[&first.meta.id]).unwrap(),
        second_bytes
    );
    std::fs::remove_file(moved.join("nested/b.mloom_fabric")).unwrap();
    let refreshed = catalog::scan(&relocated);
    assert!(catalog::resolve(&mut project, &mut library, &[refreshed])
        .iter()
        .any(|w| w.contains("Missing asset pack")));
    assert!(library.is_empty());
}
