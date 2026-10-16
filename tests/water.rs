use maploom::{assets::Library, document::*, formats, render, water};

fn style() -> WaterStyle {
    WaterStyle {
        deep_color: [55, 159, 154, 255],
        shallow_color: [141, 203, 177, 255],
        blend_distance: 1.5,
        disable_border: false,
        smoothing: 0.6,
    }
}
fn square(a: f32, b: f32) -> Vec<Point> {
    vec![
        Point::new(a, a),
        Point::new(b, a),
        Point::new(b, b),
        Point::new(a, b),
    ]
}
fn pixel(image: &water::WaterImage, x: f32, y: f32) -> [u8; 4] {
    image
        .image
        .get_pixel(
            ((x - image.origin.x) / image.size.x * image.image.width() as f32) as u32,
            ((y - image.origin.y) / image.size.y * image.image.height() as f32) as u32,
        )
        .0
}

#[test]
fn water_depth_follows_all_shores_and_leaves_islands_transparent() {
    let image = water::rasterize(&[square(0., 10.), square(4., 6.)], &style(), 96.);
    assert_eq!(pixel(&image, 5., 5.)[3], 0);
    assert_eq!(pixel(&image, -0.01, 5.)[3], 0);
    let deep = pixel(&image, 2., 2.);
    let shore = pixel(&image, 0.08, 2.);
    let island_shore = pixel(&image, 3.9, 5.);
    assert!(deep[0].abs_diff(55) < 4);
    assert!(shore[0] > deep[0] + 50);
    assert!(island_shore[0] > deep[0] + 50);
    assert_eq!(deep[3], 255);
}

#[test]
fn blend_width_zero_and_border_switch_have_visible_independent_effects() {
    let rings = vec![square(0., 8.)];
    let base = style();
    let bordered = water::rasterize(&rings, &base, 96.);
    let mut no_border = base.clone();
    no_border.disable_border = true;
    let plain = water::rasterize(&rings, &no_border, 96.);
    assert!(pixel(&plain, 0.005, 4.)[0] > pixel(&bordered, 0.005, 4.)[0] + 40);
    no_border.blend_distance = 0.;
    let no_blend = water::rasterize(&rings, &no_border, 96.);
    assert!(pixel(&plain, 0.2, 4.)[0] > pixel(&no_blend, 0.2, 4.)[0] + 40);
    no_border.blend_distance = 3.;
    let wide = water::rasterize(&rings, &no_border, 96.);
    assert!(pixel(&wide, 1.5, 4.)[0] > pixel(&plain, 1.5, 4.)[0] + 25);
}

#[test]
fn water_saves_reopens_undoes_and_exports_without_external_assets() {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut item = Item::new(
        "Water",
        ItemKind::TerrainRegion {
            rings: vec![square(1., 10.)],
            color: [255; 4],
        },
    );
    item.water = Some(style());
    p.levels[0].layers[0].items.push(item);
    p.validate().unwrap();
    let mut history = History::default();
    history.record(p.clone());
    p.levels[0].layers[0].items[0].translate(Point::new(2., 1.));
    assert!(history.undo(&mut p));
    assert!(render::hit(
        &p.levels[0].layers[0].items[0],
        Point::new(1.5, 1.5),
        0.
    ));
    let temp = tempfile::tempdir().unwrap();
    for ext in ["json", "mloom"] {
        let path = temp.path().join(format!("water.{ext}"));
        formats::save(&path, &p, &Library::new()).unwrap();
        assert_eq!(formats::load(&path).unwrap().project, p);
    }
    let svg = render::svg(
        &p,
        &Library::new(),
        render::ExportOptions {
            level: 0,
            pixels_per_grid: 64,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
    .unwrap();
    assert!(svg.contains("data:image/png;base64,"));
    p.levels[0].layers[0].items[0]
        .water
        .as_mut()
        .unwrap()
        .blend_distance = -1.;
    assert!(p.validate().is_err());
}

#[test]
fn procedural_texture_resolution_stays_bounded_for_large_maps() {
    let image = water::rasterize(&[square(0., 8192.)], &style(), 256.);
    assert!(image.image.width() <= 2048 && image.image.height() <= 2048);
}

#[test]
fn gpu_scene_uploads_water_once_and_updates_it_after_material_changes() {
    use maploom::gpu_scene::{SceneOptions, SceneRenderer};
    use std::collections::HashSet;
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut item = Item::new(
        "Water",
        ItemKind::TerrainRegion {
            rings: vec![square(1., 8.)],
            color: [255; 4],
        },
    );
    item.water = Some(style());
    p.levels[0].layers[0].items.push(item);
    let mut renderer = SceneRenderer::default();
    let options = |revision| SceneOptions {
        level: 0,
        grid: false,
        lights: false,
        revision,
        known_meshes: HashSet::new(),
    };
    let cold = renderer
        .scene(&p, &Library::new(), options(0), &HashSet::new())
        .unwrap();
    assert!(
        cold.textures
            .iter()
            .any(|t| t.width > 600 && t.height > 600)
    );
    let known = cold.used_textures.iter().cloned().collect();
    let warm = renderer
        .scene(&p, &Library::new(), options(1), &known)
        .unwrap();
    assert!(warm.textures.is_empty());
    p.levels[0].layers[0].items[0]
        .water
        .as_mut()
        .unwrap()
        .deep_color = [10, 40, 180, 255];
    let changed = renderer
        .scene(&p, &Library::new(), options(2), &known)
        .unwrap();
    assert!(changed.textures.iter().any(|t| t.width > 600));
    assert_ne!(cold.used_textures, changed.used_textures);
}
