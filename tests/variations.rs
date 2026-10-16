use maploom::{
    assets::{self, Library},
    catalog,
    document::*,
    editing, formats, render,
};
use std::{collections::HashSet, io::Read, path::Path};

fn fixture() -> Project {
    let mut p = Project {
        name: "Citadel".into(),
        background_asset: None,
        use_default_assets: false,
        width: 8,
        height: 8,
        ..Project::default()
    };
    p.main_level = Some(p.levels[0].id.clone());
    p.levels.insert(0, Level::new("Basement"));
    p.levels.push(Level::new("Tower"));
    for (index, level) in p.levels.iter_mut().enumerate() {
        level.layers[0].items.push(Item::new(
            "Marker",
            ItemKind::Text {
                position: Point::new(2., 2.),
                text: format!("Floor {index}"),
                size: 1.,
                color: [10 + index as u8, 20, 30, 255],
            },
        ));
    }
    p
}
fn options() -> render::ExportOptions {
    render::ExportOptions {
        level: 2,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    }
}
fn entries(path: &Path) -> Vec<(String, String)> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut file = zip.by_index(i).unwrap();
            assert!(file.enclosed_name().is_some());
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            (file.name().to_owned(), content)
        })
        .collect()
}

#[test]
fn copies_the_selected_variation_in_full_and_edits_stay_independent() {
    let mut p = fixture();
    p.levels[1].layers[0].locked = true;
    p.levels[1].layers[0].items[0].locked = true;
    p.levels[2].layers[0].visible = false;
    let original = p.clone();
    p.add_variation("Night".into()).unwrap();
    let night = p.active_variation.clone().unwrap();
    assert_eq!(p.levels, original.levels);
    assert_eq!(p.main_level, original.main_level);
    p.background = [1, 2, 3, 255];
    p.ambient_light = [20, 30, 40, 255];
    p.levels[2].name = "Night tower".into();
    p.levels.push(Level::new("Night rooftop"));
    let night_levels = p.levels.clone();
    p.select_variation(&original.id).unwrap();
    assert_eq!(p.levels, original.levels);
    assert_eq!(p.background, original.background);
    // Copy the original while Night is still stored: source must be the selection.
    p.add_variation("Day copy".into()).unwrap();
    assert_eq!(p.levels, original.levels);
    p.select_variation(&night).unwrap();
    assert_eq!(p.levels, night_levels);
    assert_eq!(p.background, [1, 2, 3, 255]);
    assert_eq!(p.ambient_light, [20, 30, 40, 255]);
    p.validate().unwrap();
}

#[test]
fn deletion_history_and_validation_keep_a_valid_selection() {
    let mut p = fixture();
    assert!(p.remove_variation().is_err());
    p.add_variation("Night".into()).unwrap();
    let before = p.clone();
    let mut history = History::default();
    history.record(p.clone());
    p.remove_variation().unwrap();
    assert_eq!(p.active_variation.as_deref(), Some(p.id.as_str()));
    assert_eq!(p.variations.len(), 1);
    assert!(p.remove_variation().is_err());
    assert!(history.undo(&mut p));
    assert_eq!(p, before);
    assert!(history.redo(&mut p));
    p.rename_variation("Dawn".into()).unwrap();
    assert_eq!(p.variations[0].name, "Dawn");
    p.active_variation = Some("missing".into());
    assert!(p.validate().is_err());
    let mut invalid = before.clone();
    invalid.variations[0].levels.clear();
    assert!(invalid.validate().is_err());
    let mut duplicate = before;
    duplicate.variations[0].id = duplicate.variations[1].id.clone();
    assert!(duplicate.validate().is_err());
}

#[test]
fn all_variations_and_assets_used_only_by_inactive_content_survive_save_and_open() {
    let mut p = fixture();
    let temp = tempfile::tempdir().unwrap();
    let png = temp.path().join("inactive.png");
    image::RgbaImage::from_pixel(4, 4, image::Rgba([80, 90, 100, 255]))
        .save(&png)
        .unwrap();
    let asset = assets::import_image(&png).unwrap();
    let key = asset.meta.id.clone();
    p.background_asset = Some(key.clone());
    p.add_variation("Night".into()).unwrap();
    p.background_asset = None;
    p.levels.pop();
    p.sync_active_variation();
    assert!(catalog::used_assets(&p).contains(&key));
    let library = Library::from([(key.clone(), asset)]);
    for ext in ["mloom", "json"] {
        let path = temp.path().join(format!("variations.{ext}"));
        formats::save(&path, &p, &library).unwrap();
        let loaded = formats::load(&path).unwrap();
        assert_eq!(loaded.project.levels, p.levels);
        assert_eq!(loaded.project.variations, p.variations);
        assert_eq!(loaded.project.active_variation, p.active_variation);
        assert_eq!(
            assets::verified_bytes(&loaded.library[&key]).unwrap(),
            std::fs::read(&png).unwrap()
        );
    }
}

#[test]
fn imported_object_sizes_resolve_in_every_variation_when_a_pack_becomes_available() {
    let mut p = fixture();
    let temp = tempfile::tempdir().unwrap();
    let png = temp.path().join("object.png");
    image::RgbaImage::from_pixel(256, 128, image::Rgba([80, 90, 100, 255]))
        .save(&png)
        .unwrap();
    let asset = assets::import_image(&png).unwrap();
    let object = Item::new(
        "Imported object",
        ItemKind::Object {
            asset: asset.meta.id.clone(),
            position: Point::new(2., 2.),
            size: Point::new(1., 1.),
            rotation: 0.,
            pivot: Point::new(0.5, 0.5),
            flip_x: false,
            flip_y: false,
        },
    );
    p.extensions.insert(
        "dungeondraft_object_scales".into(),
        serde_json::json!({object.id.clone(): [2, 3]}),
    );
    p.levels[0].layers[0].items = vec![object];
    p.add_variation("Night".into()).unwrap();
    let library = Library::from([(asset.meta.id.clone(), asset)]);
    formats::resolve_dd_assets(&mut p, &library).unwrap();
    for variation in &p.variations {
        let ItemKind::Object { size, .. } =
            p.variation_scene(variation).levels[0].layers[0].items[0].kind
        else {
            panic!()
        };
        assert_eq!(size, Point::new(2., 1.5));
    }
}

#[test]
fn resize_moves_every_variation_once_and_preserves_independent_floor_sets() {
    let mut p = fixture();
    p.add_variation("Night".into()).unwrap();
    p.levels.pop();
    editing::resize_map(&mut p, 2, 0, 0, 3).unwrap();
    for v in &p.variations {
        let scene = p.variation_scene(v);
        assert_eq!(
            scene.levels[0].layers[0].items[0].points()[0],
            Point::new(5., 4.)
        );
    }
    assert_eq!(p.variations[0].levels.len(), 3);
    assert_eq!(p.variations[1].levels.len(), 2);
}

#[test]
fn export_modes_keep_variations_and_floors_separate_with_safe_unique_names() {
    let mut p = fixture();
    p.add_variation("../Night:?".into()).unwrap();
    p.background = [11, 22, 33, 255];
    p.levels.pop();
    p.add_variation("../NIGHT:?".into()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    let single = temp.path().join("Citadel.zip");
    render::export_variations(&single, &p, &Library::new(), options(), "svg", false).unwrap();
    let images = entries(&single);
    assert_eq!(images.len(), 3);
    assert_eq!(
        images
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        [
            "Citadel-Variation 1.svg",
            "Citadel-_Night__.svg",
            "Citadel-_NIGHT__-2.svg",
        ]
    );
    for (name, svg) in &images {
        assert!(!name.contains(['/', '\\', ':', '?']));
        assert!(svg.contains("width=\"256\" height=\"256\""));
        // Main floor only, despite a different active floor in options().
        assert!(svg.contains("fill=\"#0b141e\""));
        assert!(!svg.contains("fill=\"#0a141e\""));
        assert!(!svg.contains("fill=\"#0c141e\""));
    }
    assert!(images[1].1.contains("fill=\"#0b1621\""));
    let folder = temp.path().join("both");
    render::export_variations(&folder, &p, &Library::new(), options(), "svg", true).unwrap();
    let names = render::variation_archive_names(&p);
    assert_eq!(names.len(), 3);
    assert_eq!(
        names
            .iter()
            .map(|s| s.to_lowercase())
            .collect::<HashSet<_>>()
            .len(),
        3
    );
    for (i, name) in names.iter().enumerate() {
        assert!(name.starts_with("Citadel-"));
        assert!(!name.contains(['/', '\\', ':', '?']));
        let floors = entries(&folder.join(name));
        assert_eq!(floors.len(), if i == 0 { 3 } else { 2 });
        let variation = ["Variation 1", "_Night__", "_NIGHT__"][i];
        assert_eq!(floors[0].0, format!("Citadel-Basement-{variation}.svg"));
        for (j, (file_name, svg)) in floors.iter().enumerate() {
            assert!(file_name.starts_with("Citadel-"));
            assert!(file_name.ends_with(&format!("-{variation}.svg")));
            assert!(svg.contains(if j == 1 {
                "fill-opacity=\"1\""
            } else {
                "fill-opacity=\"0\""
            }));
        }
    }
    // Floor-only mode still exports the current variation, including its own count.
    let floors = temp.path().join("floors.zip");
    render::export_all(&floors, &p, &Library::new(), options(), "svg").unwrap();
    assert_eq!(entries(&floors).len(), 2);
    assert_eq!(entries(&floors)[0].0, "Citadel-Basement.svg");
}

#[test]
fn failed_variation_export_keeps_existing_archives_and_legacy_maps_export_as_one_variation() {
    let mut p = fixture();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("map.zip");
    render::export_variations(&path, &p, &Library::new(), options(), "svg", false).unwrap();
    assert_eq!(entries(&path).len(), 1);
    p.add_variation("Night".into()).unwrap();
    p.background_asset = Some("missing-asset".into());
    std::fs::write(&path, b"original").unwrap();
    assert!(
        render::export_variations(&path, &p, &Library::new(), options(), "svg", false).is_err()
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    let names = render::variation_archive_names(&p);
    for name in &names {
        std::fs::write(temp.path().join(name), b"original").unwrap();
    }
    assert!(
        render::export_variations(temp.path(), &p, &Library::new(), options(), "svg", true)
            .is_err()
    );
    for name in &names {
        assert_eq!(std::fs::read(temp.path().join(name)).unwrap(), b"original");
    }
    assert!(render::validate_variation_export(&fixture(), true, "jpeg").is_err());
    assert!(render::validate_variation_export(&fixture(), false, "jpeg").is_ok());
}
