use base64::{engine::general_purpose::STANDARD, Engine};
use maploom::{
    assets::Library,
    document::*,
    formats,
    gpu_scene::{GpuScene, SceneOptions, SceneRenderer},
    render,
};
use std::collections::HashSet;

fn fixture() -> Project {
    let mut project = Project {
        width: 8,
        height: 8,
        background: [120, 150, 180, 255],
        background_asset: None,
        use_default_assets: false,
        ..Project::default()
    };
    project.main_level = Some(project.levels[0].id.clone());
    project.levels.insert(0, Level::new("Basement"));
    project.levels.push(Level::new("First"));
    project.levels.push(Level::new("Second"));
    project
}
fn options(level: usize) -> render::ExportOptions {
    render::ExportOptions {
        level,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    }
}
fn scene(renderer: &mut SceneRenderer, project: &Project, level: usize) -> GpuScene {
    renderer
        .scene(
            project,
            &Library::new(),
            SceneOptions {
                level,
                grid: false,
                lights: false,
                revision: 0,
                known_meshes: HashSet::new(),
            },
            &HashSet::new(),
        )
        .unwrap()
}
fn colors(scene: &GpuScene) -> Vec<[f32; 4]> {
    scene
        .meshes
        .iter()
        .flat_map(|mesh| {
            STANDARD
                .decode(&mesh.vertices)
                .unwrap()
                .chunks_exact(48)
                .map(|vertex| {
                    std::array::from_fn(|i| {
                        f32::from_le_bytes(vertex[16 + i * 4..20 + i * 4].try_into().unwrap())
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn commits_cannot_remove_or_move_the_main_floor_or_cross_it() {
    let p = fixture();
    let mut moved = p.clone();
    moved.levels.swap(1, 2);
    assert!(p.protect_main_floor(&mut moved).is_err());
    let mut removed = p.clone();
    removed.levels.remove(1);
    assert!(p.protect_main_floor(&mut removed).is_err());
    let mut replaced = p.clone();
    replaced.main_level = Some(p.levels[2].id.clone());
    assert!(p.protect_main_floor(&mut replaced).is_err());
    let mut added = p.clone();
    added.levels.insert(0, Level::new("Deeper basement"));
    assert!(p.protect_main_floor(&mut added).is_ok());
    assert_eq!(added.main_level_index(), 2);
    let mut reordered = p.clone();
    reordered.levels.swap(2, 3);
    assert!(p.protect_main_floor(&mut reordered).is_ok());
}

#[test]
fn main_floor_reference_survives_files_and_history_and_legacy_defaults_to_first() {
    let mut p = fixture();
    assert_eq!(p.main_level_index(), 1);
    let mut legacy = serde_json::to_value(&p).unwrap();
    legacy.as_object_mut().unwrap().remove("main_level");
    assert_eq!(
        serde_json::from_value::<Project>(legacy)
            .unwrap()
            .main_level_index(),
        0
    );
    let temp = tempfile::tempdir().unwrap();
    for ext in ["mloom", "json"] {
        let path = temp.path().join(format!("floors.{ext}"));
        formats::save(&path, &p, &Library::new()).unwrap();
        assert_eq!(formats::load(&path).unwrap().project, p);
    }
    let original = p.clone();
    let mut history = History::default();
    history.record(p.clone());
    p.levels.swap(2, 3);
    let reordered = p.clone();
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(history.redo(&mut p));
    assert_eq!(p, reordered);
    p.main_level = Some("missing".into());
    assert!(p.validate().is_err());
}

#[test]
fn gpu_surfaces_have_isolated_basement_main_background_and_transparent_upper_floors() {
    let p = fixture();
    let mut renderer = SceneRenderer::default();
    let basement = scene(&mut renderer, &p, 0);
    assert_eq!(basement.underlay_meshes, 0);
    assert!(colors(&basement).iter().all(|c| c[..3] == [0.; 3]));
    assert!(colors(&basement).iter().any(|c| c[3] == 1.));
    let main = scene(&mut renderer, &p, 1);
    assert_eq!(main.underlay_meshes, 0);
    assert!(colors(&main).iter().any(|c| c[0] > 0. && c[3] == 1.));
    let top = scene(&mut renderer, &p, 3);
    assert!(top.underlay_meshes >= main.meshes.len());
    assert!(top.text_bounds.is_empty());
    // A basement does not depend on the main floor's terrain resource.
    let mut missing_terrain = p.clone();
    missing_terrain.background_asset = Some("missing-terrain".into());
    scene(&mut renderer, &missing_terrain, 0);
}

#[test]
fn upper_underlay_keeps_per_floor_order_and_omits_hidden_content_and_selection_bounds() {
    let mut p = fixture();
    for index in 1..=2 {
        p.levels[index].layers[0].items.push(Item::new(
            format!("Lower {index}"),
            ItemKind::Text {
                position: Point::new(2., 2.),
                text: format!("Floor {index}"),
                size: 1.,
                color: [255; 4],
            },
        ));
    }
    let mut renderer = SceneRenderer::default();
    let before = scene(&mut renderer, &p, 3);
    assert!(before.underlay_meshes > 1);
    assert!(before.text_bounds.is_empty());
    p.levels[2].layers[0].visible = false;
    let hidden = scene(&mut renderer, &p, 3);
    assert!(hidden.underlay_meshes < before.underlay_meshes);
    p.levels[2].layers[0].visible = true;
    p.levels[1].layers[0].items[0].translate(Point::new(1., 0.));
    let moved = scene(&mut renderer, &p, 3);
    assert_ne!(
        before.meshes.iter().map(|m| &m.key).collect::<Vec<_>>(),
        moved.meshes.iter().map(|m| &m.key).collect::<Vec<_>>()
    );
}

#[test]
fn svg_exports_only_selected_floor_and_retains_only_the_main_background() {
    let mut p = fixture();
    for (index, floor) in p.levels.iter_mut().enumerate() {
        floor.layers[0].items.push(Item::new(
            "Label",
            ItemKind::Text {
                position: Point::new(2., 2.),
                text: format!("OnlyFloor{index}"),
                size: 1.,
                color: [10 + index as u8, 20, 30, 255],
            },
        ));
    }
    for level in 0..p.levels.len() {
        let svg = render::svg(&p, &Library::new(), options(level)).unwrap();
        assert!(!svg.contains("floor-underlay"));
        assert!(!svg.contains("feGaussianBlur"));
        assert!(svg.contains(&format!("fill=\"#{:02x}141e\"", 10 + level)));
        for other in 0..p.levels.len() {
            if other != level {
                assert!(!svg.contains(&format!("fill=\"#{:02x}141e\"", 10 + other)));
            }
        }
        if level != p.main_level_index() {
            assert!(svg.contains("fill-opacity=\"0\""));
        } else {
            assert!(svg.contains("fill=\"#7896b4\" fill-opacity=\"1\""));
        }
    }
    // A missing terrain asset on the main floor cannot block another floor's export.
    p.background_asset = Some("missing-main-terrain".into());
    assert!(render::svg(&p, &Library::new(), options(0)).is_ok());
    assert!(render::svg(&p, &Library::new(), options(3)).is_ok());
}

#[test]
fn batch_svg_archive_has_safe_unique_names_full_canvases_and_independent_floor_content() {
    use std::io::Read;
    let mut p = fixture();
    for (index, floor) in p.levels.iter_mut().enumerate() {
        floor.name = "../Same: имя?".into();
        floor.layers[0].items.push(Item::new(
            "Label",
            ItemKind::Text {
                position: Point::new(2., 2.),
                text: format!("OnlyFloor{index}"),
                size: 1.,
                color: [10 + index as u8, 20, 30, 255],
            },
        ));
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("floors.zip");
    render::export_all(&path, &p, &Library::new(), options(0), "svg").unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    assert_eq!(zip.len(), p.levels.len());
    let mut names = HashSet::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).unwrap();
        assert!(entry.enclosed_name().is_some());
        assert!(!entry.name().contains(['/', '\\', ':', '?']));
        let suffix = if index == 0 {
            String::new()
        } else {
            format!("-{}", index + 1)
        };
        assert_eq!(
            entry.name(),
            format!("Untitled map-_Same_ имя_{suffix}.svg")
        );
        assert!(names.insert(entry.name().to_owned()));
        let mut svg = String::new();
        entry.read_to_string(&mut svg).unwrap();
        assert!(svg.contains("width=\"256\" height=\"256\""));
        assert!(svg.contains(&format!("fill=\"#{:02x}141e\"", 10 + index)));
        for other in 0..p.levels.len() {
            if other != index {
                assert!(!svg.contains(&format!("fill=\"#{:02x}141e\"", 10 + other)));
            }
        }
        assert!(!svg.contains("feGaussianBlur"));
        assert!(svg.contains(if index == 1 {
            "fill-opacity=\"1\""
        } else {
            "fill-opacity=\"0\""
        }));
    }
}

#[test]
fn failed_batch_keeps_existing_output_and_rejects_formats_without_transparency() {
    let mut p = fixture();
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("floors.zip");
    std::fs::write(&path, b"previous archive").unwrap();
    let mut missing = Item::new(
        "Missing",
        ItemKind::Text {
            position: Point::new(1., 1.),
            text: "Missing".into(),
            size: 1.,
            color: [255; 4],
        },
    );
    missing.texture = Some("missing".into());
    p.levels.last_mut().unwrap().layers[0].items.push(missing);
    assert!(render::export_all(&path, &p, &Library::new(), options(0), "svg").is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous archive");
    for format in ["jpeg", "jpg", "unsupported"] {
        assert!(render::export_all(&path, &p, &Library::new(), options(0), format).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous archive");
    }
    let jpeg = temp.path().join("basement.jpeg");
    assert!(render::export(&jpeg, &p, &Library::new(), options(0)).is_err());
    assert!(!jpeg.exists());
}

#[test]
#[ignore = "Requires a hardware GPU; run explicitly for native export verification"]
fn native_exports_preserve_main_background_and_other_floor_transparency() {
    let mut p = fixture();
    let (basement, gpu) = render::rasterize(&p, &Library::new(), options(0)).unwrap();
    assert_eq!(basement.get_pixel(128, 128).0, [0; 4]);
    let (upper, _) = render::rasterize(&p, &Library::new(), options(3)).unwrap();
    assert_eq!(upper.get_pixel(128, 128).0, [0; 4]);
    let (main, _) = render::rasterize(&p, &Library::new(), options(1)).unwrap();
    assert_eq!(main.get_pixel(128, 128).0, p.background);
    p.levels[3].layers[0].items.push(Item::new(
        "Light",
        ItemKind::Light {
            position: Point::new(4., 4.),
            range: 2.,
            intensity: 1.,
            smoothing: 1.,
            color: [255; 4],
            shadows: false,
        },
    ));
    let (lit, _) = render::rasterize(
        &p,
        &Library::new(),
        render::ExportOptions {
            lights: true,
            ..options(3)
        },
    )
        .unwrap();
    assert!(lit.get_pixel(128, 128)[0] > upper.get_pixel(128, 128)[0]);

    use maploom::assets::{self, Asset, Source};
    use std::{io::Cursor, sync::Arc};
    let mut png = Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([255; 4]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    let tile = Asset {
        meta: AssetMeta {
            id: "tile".into(),
            name: "Tile".into(),
            path: "tile.png".into(),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(bytes)),
    };
    let library = Library::from([("tile".into(), tile)]);
    let square = |x: f32, y: f32, width: f32, height: f32, tint| {
        let mut item = Item::new(
            "Square",
            ItemKind::Floor {
                points: vec![
                    Point::new(x, y),
                    Point::new(x + width, y),
                    Point::new(x + width, y + height),
                    Point::new(x, y + height),
                ],
                color: [255; 4],
            },
        )
            .with_texture(Some("tile"));
        item.tint = tint;
        item
    };
    p.levels[1].layers[0]
        .items
        .push(square(2., 2., 2., 4., [230, 20, 20, 255]));
    p.levels[2].layers[0]
        .items
        .push(square(3., 3., 2., 2., [20, 240, 20, 255]));
    p.levels[3].layers[0]
        .items
        .push(square(5., 1., 2., 2., [20, 20, 240, 255]));
    p.levels[0].layers[0]
        .items
        .push(square(5., 1., 2., 2., [20, 20, 240, 255]));
    let (painted, _) = render::rasterize(&p, &library, options(3)).unwrap();
    let overlap = painted.get_pixel(112, 128).0;
    assert_eq!(
        overlap, [0; 4],
        "No content from lower floors appears in the export"
    );
    assert_eq!(
        painted.get_pixel(192, 64).0,
        [20, 20, 240, 255],
        "Selected content stays sharp and undimmed"
    );
    assert_eq!(painted.get_pixel(63, 128).0, [0; 4]);
    assert_eq!(painted.get_pixel(80, 128).0, [0; 4]);
    let (painted_basement, _) = render::rasterize(&p, &library, options(0)).unwrap();
    assert_eq!(painted_basement.get_pixel(192, 64).0, [20, 20, 240, 255]);
    assert_eq!(painted_basement.get_pixel(32, 32).0, [0; 4]);
    // Exercise actual encoders and the batch archive with the same registration and alpha.
    use std::io::Read;
    let temp = tempfile::tempdir().unwrap();
    for format in ["png", "webp", "dd2vtt"] {
        let path = temp.path().join(format!("{format}.zip"));
        render::export_all(&path, &p, &library, options(0), format).unwrap();
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        assert_eq!(archive.len(), p.levels.len());
        for level in 0..p.levels.len() {
            let mut data = Vec::new();
            archive
                .by_index(level)
                .unwrap()
                .read_to_end(&mut data)
                .unwrap();
            if format == "dd2vtt" {
                let vtt: serde_json::Value = serde_json::from_slice(&data).unwrap();
                data = STANDARD.decode(vtt["image"].as_str().unwrap()).unwrap();
            }
            let image = image::load_from_memory(&data).unwrap().to_rgba8();
            assert_eq!(image.dimensions(), (256, 256));
            assert_eq!(
                image.get_pixel(32, 32).0,
                if level == 1 { p.background } else { [0; 4] }
            );
            if level == 0 || level == 3 {
                assert_eq!(image.get_pixel(192, 64).0, [20, 20, 240, 255]);
            }
        }
    }
    println!("{gpu}; native floor alpha and PNG/WebP/VTT batch archives verified");
}
