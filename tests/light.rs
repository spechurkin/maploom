use base64::{Engine, engine::general_purpose::STANDARD};
use maploom::{
    assets::Library,
    document::*,
    formats,
    gpu_scene::{SceneOptions, SceneRenderer},
    light, render,
};
use std::collections::HashSet;

fn fixture(smoothing: f32) -> Project {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    p.levels[0].layers[0].items.push(Item::new(
        "Light",
        ItemKind::Light {
            position: Point::new(5., 5.),
            range: 3.,
            intensity: 1.7,
            color: [75, 155, 255, 180],
            shadows: true,
            smoothing,
        },
    ));
    p
}

#[test]
fn old_projects_default_to_soft_lights_and_new_settings_roundtrip() {
    let mut p = fixture(0.37);
    let mut old = serde_json::to_value(&p).unwrap();
    old["levels"][0]["layers"][0]["items"][0]["kind"]
        .as_object_mut()
        .unwrap()
        .remove("smoothing");
    let old: Project = serde_json::from_value(old).unwrap();
    assert!(matches!(
        old.levels[0].layers[0].items[0].kind,
        ItemKind::Light { smoothing: 1., .. }
    ));
    old.validate().unwrap();
    let temp = tempfile::tempdir().unwrap();
    for ext in ["json", "mloom"] {
        let path = temp.path().join(format!("light.{ext}"));
        formats::save(&path, &p, &Library::new()).unwrap();
        assert_eq!(formats::load(&path).unwrap().project, p);
    }
    let original = p.clone();
    let mut history = History::default();
    history.record(p.clone());
    if let ItemKind::Light {
        smoothing,
        range,
        intensity,
        color,
        ..
    } = &mut p.levels[0].layers[0].items[0].kind
    {
        *smoothing = 0.;
        *range = 8.;
        *intensity = 0.;
        *color = [255, 20, 15, 255];
    }
    let changed = p.clone();
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(history.redo(&mut p));
    assert_eq!(p, changed);
    for invalid in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
        if let ItemKind::Light { smoothing, .. } = &mut p.levels[0].layers[0].items[0].kind {
            *smoothing = invalid;
        }
        assert!(p.validate().is_err());
    }
}

#[test]
fn smoothing_removes_bands_and_angular_edges_without_moving_the_source() {
    let hard = light::rasterize(0., 1., 512);
    let middle = light::rasterize(0.5, 1., 512);
    let soft = light::rasterize(1., 1., 512);
    let levels =
        |image: &image::RgbaImage| image.pixels().map(|p| p[3]).collect::<HashSet<_>>().len();
    assert_eq!(
        levels(&hard),
        6,
        "Five flat bands plus the transparent exterior"
    );
    assert!(levels(&middle) > 50);
    assert!(levels(&soft) > 100);
    let max_jump = |image: &image::RgbaImage| {
        (256..510)
            .map(|x| image.get_pixel(x, 256)[3].abs_diff(image.get_pixel(x + 1, 256)[3]))
            .max()
            .unwrap()
    };
    assert!(max_jump(&hard) > 40);
    assert!(max_jump(&soft) <= 2);
    assert!(
        (256..512).any(|x| hard.get_pixel(x, 256)[3] != hard.get_pixel(256, x)[3]),
        "Hard contours are irregular"
    );
    for x in 256..512 {
        assert_eq!(
            soft.get_pixel(x, 256)[3],
            soft.get_pixel(256, x)[3],
            "Soft glow is radial"
        );
    }
    assert_eq!(
        hard,
        light::rasterize(0., 1., 512),
        "Repainting must not flicker"
    );
}

#[test]
fn intensity_zero_removes_every_pixel_and_values_above_two_still_matter() {
    assert!(light::rasterize(0.4, 0., 64).pixels().all(|p| p[3] == 0));
    let dim = light::rasterize(1., 0.3, 64);
    let normal = light::rasterize(1., 1., 64);
    let bright = light::rasterize(1., 3., 64);
    let brightest = light::rasterize(1., 8., 64);
    for (x, y) in [(32, 32), (48, 32)] {
        assert!(dim.get_pixel(x, y)[3] < normal.get_pixel(x, y)[3]);
        assert!(normal.get_pixel(x, y)[3] < bright.get_pixel(x, y)[3]);
        assert!(bright.get_pixel(x, y)[3] < brightest.get_pixel(x, y)[3]);
    }
}

#[test]
fn svg_embeds_the_same_light_profile_color_alpha_and_radius_and_honors_visibility() {
    let mut p = fixture(0.37);
    // This legacy/shared tint must not multiply the independently chosen light color.
    p.levels[0].layers[0].items[0].tint = [10, 20, 30, 255];
    let mut options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 64,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    let svg = render::svg(&p, &Library::new(), options).unwrap();
    assert!(svg.contains("<image x=\"2\" y=\"2\" width=\"6\" height=\"6\""));
    let png = svg
        .split("data:image/png;base64,")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let image = image::load_from_memory(&STANDARD.decode(png).unwrap())
        .unwrap()
        .to_rgba8();
    let stamp = light::rasterize(0.37, 1.7, 384);
    for (actual, expected) in image.pixels().zip(stamp.pixels()) {
        assert_eq!(actual.0, multiply_colors(expected.0, [75, 155, 255, 180]));
    }
    options.lights = false;
    assert!(
        !render::svg(&p, &Library::new(), options)
            .unwrap()
            .contains("data:image/png;base64,")
    );
    options.lights = true;
    p.levels[0].layers[0].visible = false;
    assert!(
        !render::svg(&p, &Library::new(), options)
            .unwrap()
            .contains("data:image/png;base64,")
    );
    let vtt = formats::dd2vtt(&fixture(0.), 0, 64, b"PNG", &Library::new());
    assert_eq!(vtt["lights"][0]["range"], 3.);
    assert_eq!(vtt["lights"][0]["color"], "b44b9bff");
}

#[test]
fn gpu_reuses_stamps_for_color_size_and_movement_but_updates_smoothing() {
    let mut p = fixture(0.);
    let mut renderer = SceneRenderer::default();
    let options = |revision, lights| SceneOptions {
        level: 0,
        grid: false,
        lights,
        revision,
        known_meshes: HashSet::new(),
    };
    let first = renderer
        .scene(&p, &Library::new(), options(0, true), &HashSet::new())
        .unwrap();
    assert!(
        first
            .textures
            .iter()
            .any(|t| t.width == 512 && t.height == 512)
    );
    let known = first.used_textures.iter().cloned().collect();
    if let ItemKind::Light {
        color,
        range,
        position,
        ..
    } = &mut p.levels[0].layers[0].items[0].kind
    {
        *color = [255, 50, 20, 255];
        *range = 5.;
        position.x += 1.;
    }
    let moved = renderer
        .scene(&p, &Library::new(), options(1, true), &known)
        .unwrap();
    assert!(moved.textures.is_empty());
    let stamp_key = &first
        .textures
        .iter()
        .find(|t| t.width == 512 && t.height == 512)
        .unwrap()
        .key;
    assert_ne!(
        first
            .meshes
            .iter()
            .find(|m| &m.texture == stamp_key)
            .unwrap()
            .key,
        moved
            .meshes
            .iter()
            .find(|m| &m.texture == stamp_key)
            .unwrap()
            .key
    );
    if let ItemKind::Light { smoothing, .. } = &mut p.levels[0].layers[0].items[0].kind {
        *smoothing = 1.;
    }
    let soft = renderer
        .scene(&p, &Library::new(), options(2, true), &known)
        .unwrap();
    assert!(
        soft.textures
            .iter()
            .any(|t| t.width == 512 && t.height == 512)
    );
    assert!(
        renderer
            .scene(&p, &Library::new(), options(3, false), &known)
            .unwrap()
            .textures
            .is_empty()
    );
}
