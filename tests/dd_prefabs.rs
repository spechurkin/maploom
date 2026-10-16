use maploom::{
    assets::{self, Library},
    document::*,
    formats,
    storage::{
        dd_prefabs::{self, Definition},
        prefabs,
    },
};
use std::collections::BTreeSet;

fn definitions() -> Vec<Definition> {
    serde_json::from_str(include_str!("../resources/default-prefabs.json")).unwrap()
}
fn library() -> Library {
    assets::index_pack(&assets::default_fabric_path().unwrap())
        .unwrap()
        .assets
        .into_iter()
        .map(|a| (a.meta.id.clone(), a))
        .collect()
}

#[test]
fn all_fifteen_original_dd_examples_preserve_objects_and_every_auxiliary_component() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let mut library = library();
    let expected: BTreeSet<_> = [
        "Altar Bountiful",
        "Altar Forgotten",
        "Altar Sacrificial",
        "Caravan Wagon Basic",
        "Caravan Wagon Full",
        "Crane",
        "Crates",
        "Fallen Tree",
        "Market Set",
        "Posh Gondola",
        "Smithing Set",
        "Tavern Tables",
        "Water Wheel",
        "Well",
        "Wooden Raft",
    ]
        .into_iter()
        .collect();
    assert_eq!(
        pack.prefabs
            .iter()
            .map(|p| p.name.as_str())
            .collect::<BTreeSet<_>>(),
        expected
    );
    for definition in definitions() {
        let converted = dd_prefabs::parse(&definition, &mut library).unwrap();
        let prefab = pack
            .prefabs
            .iter()
            .find(|p| p.name == definition.name)
            .unwrap();
        assert_eq!(
            converted.items, prefab.items,
            "{} must match the original data",
            definition.name
        );
        assert!(library.contains_key(prefab.preview.as_deref().unwrap()));
        let raw = &definition.data;
        let mut source_objects = raw["objects"]
            .as_array()
            .unwrap()
            .iter()
            .collect::<Vec<_>>();
        source_objects.sort_by_key(|o| o["layer"].as_i64().unwrap_or(100));
        let objects = prefab
            .items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Object { .. }))
            .collect::<Vec<_>>();
        assert_eq!(objects.len(), source_objects.len());
        let mut offset = None;
        for (source, object) in source_objects.into_iter().zip(objects) {
            let ItemKind::Object {
                asset,
                position,
                size,
                rotation,
                flip_x,
                flip_y,
                ..
            } = &object.kind
            else {
                unreachable!()
            };
            assert_eq!(asset, source["texture"].as_str().unwrap());
            assert!((rotation - source["rotation"].as_f64().unwrap_or(0.) as f32).abs() < 0.00001);
            let scale = formats::dd_point(&source["scale"], 1.);
            assert_eq!(
                *size,
                Point::new(
                    library[asset].meta.size.x * scale.x.abs(),
                    library[asset].meta.size.y * scale.y.abs()
                )
            );
            assert_eq!(*flip_x, source["mirror"].as_bool().unwrap_or(scale.x < 0.));
            assert_eq!(*flip_y, scale.y < 0.);
            assert_eq!(
                object.tint,
                formats::dd_color(&source["custom_color"], [255; 4])
            );
            let original = formats::dd_point(&source["position"], 256.);
            let difference = Point::new(position.x - original.x, position.y - original.y);
            if let Some(offset) = offset {
                assert!(
                    difference.distance(offset) < 0.0001,
                    "relative layout changed: {}",
                    definition.name
                );
            } else {
                offset = Some(difference);
            }
        }
        let shapes = raw["pattern_shapes"].as_array().map_or(0, Vec::len);
        let outlines = raw["pattern_shapes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["outline"] == true)
            .count();
        let paths = raw["pathways"].as_array().map_or(0, Vec::len);
        assert_eq!(
            prefab
                .items
                .iter()
                .filter(|i| matches!(i.kind, ItemKind::Floor { .. }))
                .count(),
            shapes
        );
        assert_eq!(
            prefab
                .items
                .iter()
                .filter(|i| matches!(i.kind, ItemKind::Path { .. }))
                .count(),
            paths + outlines
        );
        prefab.validate().unwrap();
    }
}

#[test]
fn mloom_prefab_is_self_contained_and_roundtrips_mixed_geometry_and_previews() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let originals = pack
        .prefabs
        .iter()
        .filter(|p| {
            ["Caravan Wagon Full", "Crates", "Wooden Raft", "Well"].contains(&p.name.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let (prefabs, images) = prefabs::portable(&originals, &library()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let export = temp.path().join("originals.mloom_prefab");
    assets::save_prefab_file(&export, "Originals", &images, &prefabs).unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&export).unwrap()).unwrap();
    assert!(zip.by_name("fabric.json").is_err());
    let manifest: serde_json::Value =
        serde_json::from_reader(zip.by_name("prefab.json").unwrap()).unwrap();
    assert_eq!(manifest["format"], "maploom/prefab");
    drop(zip);
    let moved = temp.path().join("another-computer.mloom_prefab");
    std::fs::rename(export, &moved).unwrap();
    let store = prefabs::Store::load(&moved).unwrap();
    assert_eq!(store.personal, prefabs);
    for (original, imported) in originals.iter().zip(&store.personal) {
        assert_eq!(original.items.len(), imported.items.len());
        assert!(store
            .assets
            .contains_key(imported.preview.as_deref().unwrap()));
        let placed = imported.place(Point::new(20., 20.), 1.5, 0.4).unwrap();
        for (source, copy) in imported.items.iter().zip(&placed) {
            assert_eq!(source.path_profile, copy.path_profile);
            assert_eq!(source.floor_outline, copy.floor_outline);
        }
        let mut project = Project {
            background_asset: None,
            ..Project::default()
        };
        project.levels[0].layers[1].items = placed;
        project.validate().unwrap();
        assets::require_assets(&project, 0, &store.assets).unwrap();
        let map = temp.path().join(format!("{}.mloom", imported.name));
        formats::save(&map, &project, &store.assets).unwrap();
        let loaded = formats::load(&map).unwrap();
        assert_eq!(loaded.project.levels, project.levels);
    }
}

#[test]
fn raw_dd_import_requires_dependencies_and_never_silently_drops_unsupported_components() {
    let definition = definitions()
        .into_iter()
        .find(|d| d.name == "Crates")
        .unwrap();
    let error = dd_prefabs::parse(&definition, &mut Library::new()).unwrap_err();
    assert!(error.to_string().contains("Missing prefab asset"));
    let mut unsupported = definition;
    unsupported.data["lights"] = serde_json::json!([{"range": 1}]);
    assert!(dd_prefabs::parse(&unsupported, &mut library())
        .unwrap_err()
        .to_string()
        .contains("Unsupported"));
}

#[test]
fn personal_fabric_library_migrates_to_the_dedicated_prefab_extension() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let (prefabs, images) = prefabs::portable(&pack.prefabs[..1], &library()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("prefabs.mloom_fabric");
    let new = temp.path().join("prefabs.mloom_prefab");
    assets::save_fabric_with_prefabs(&old, "Old library", "", &images, &prefabs).unwrap();
    let store = prefabs::Store::load(&new).unwrap();
    assert_eq!(store.personal, prefabs);
    store.save(&new).unwrap();
    assert!(old.is_file());
    assert_eq!(prefabs::Store::load(&new).unwrap().personal, prefabs);
}
