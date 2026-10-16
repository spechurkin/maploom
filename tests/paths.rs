use maploom::{
    assets::{self, Library},
    document::*,
    formats,
    gpu_scene::{SceneOptions, SceneRenderer},
    paths::{self, PathStyle, Transition},
    render,
};
use std::collections::HashSet;
fn points() -> Vec<Point> {
    vec![
        Point::new(2., 4.),
        Point::new(8., 4.),
        Point::new(8., 10.),
        Point::new(14., 10.),
    ]
}
fn path() -> Item {
    let mut i = Item::new(
        "Path",
        ItemKind::Path {
            points: points(),
            width: 0.5,
            color: [255; 4],
        },
    )
    .with_texture(Some("res://textures/paths/path_blender_03.png"));
    i.path_style = Some(PathStyle {
        smoothness: 1.,
        ..Default::default()
    });
    i.texture_scale = 2.;
    i
}
fn options() -> render::ExportOptions {
    render::ExportOptions {
        level: 0,
        pixels_per_grid: 64,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    }
}
#[test]
fn smoothing_rounds_corners_without_blurring_and_uvs_follow_the_entire_curve() {
    let sharp = paths::sample(&points(), 0.);
    let smooth = paths::sample(&points(), 1.);
    assert_eq!(smooth.first(), points().first());
    assert_eq!(smooth.last(), points().last());
    assert!(sharp.iter().all(|p| p.y == 4. || p.x == 8. || p.y == 10.));
    assert!(smooth.iter().any(|p| p.y < 3.8));
    let ribbon = paths::item_ribbon(&path());
    assert!(
        ribbon
            .iter()
            .all(|v| v.alpha == 1. && (v.uv.y == 0. || v.uv.y == 1.) && v.position.finite())
    );
    assert!(ribbon.windows(2).all(|v| v[1].uv.x >= v[0].uv.x));
    let total: f32 = smooth.windows(2).map(|p| p[0].distance(p[1])).sum();
    assert!((ribbon.last().unwrap().uv.x - total / 2.).abs() < 1e-5);
}
#[test]
fn transitions_are_independent_and_selection_follows_the_rendered_curve() {
    let ps = [Point::new(0., 0.), Point::new(10., 0.)];
    let style = PathStyle {
        transition_in: Transition::Fade,
        transition_out: Transition::Shrink,
        ..Default::default()
    };
    let ribbon = paths::ribbon(&ps, 1., 2., &style);
    assert_eq!(ribbon[0].alpha, 0.);
    assert_eq!(ribbon[0].position.y, 0.5);
    assert_eq!(ribbon.last().unwrap().alpha, 1.);
    assert_eq!(ribbon.last().unwrap().position.y, 0.);
    assert!(ribbon.iter().any(|v| v.alpha > 0. && v.alpha < 1.));
    let long = paths::ribbon(
        &[Point::new(0., 0.), Point::new(512., 0.)],
        0.1,
        2.,
        &PathStyle {
            transition_in: Transition::Fade,
            ..Default::default()
        },
    );
    assert!(
        long.iter()
            .any(|v| (v.position.x - 0.05).abs() < 1e-6 && (v.alpha - 0.5).abs() < 1e-5)
    );
    assert!(
        long.iter()
            .filter(|v| v.position.x >= 0.1)
            .all(|v| v.alpha == 1.)
    );
    let i = path();
    let ps = paths::sample(&points(), 1.);
    let p = ps[10];
    assert!(render::hit(&i, p, 0.));
    assert!(!render::hit(&i, Point::new(20., 20.), 0.));
    assert!(render::intersects_box(
        &i,
        Point::new(p.x - 0.01, p.y - 0.01),
        Point::new(p.x + 0.01, p.y + 0.01)
    ));
}
#[test]
fn path_style_roundtrips_and_block_light_controls_vtt_and_light_visibility() {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut i = path();
    i.texture = None;
    p.levels[0].layers[1].items.push(i);
    assert!(formats::line_of_sight(&p.levels[0]).is_empty());
    p.levels[0].layers[1].items[0]
        .path_style
        .as_mut()
        .unwrap()
        .blocks_light = true;
    assert_eq!(
        formats::line_of_sight(&p.levels[0]).len(),
        paths::sample(&points(), 1.).len() - 1
    );
    assert!(
        maploom::light::Blockers::new(&p, 0, &Library::new())
            .visibility(Point::new(4., 2.), 20.)
            .is_some()
    );
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paths.mloom");
    formats::save(&file, &p, &Library::new()).unwrap();
    assert_eq!(formats::load(&file).unwrap().project.levels, p.levels);
    p.levels[0].layers[1].visible = false;
    assert!(formats::line_of_sight(&p.levels[0]).is_empty());
    let mut value = serde_json::to_value(path()).unwrap();
    value.as_object_mut().unwrap().remove("path_style");
    assert!(
        serde_json::from_value::<Item>(value)
            .unwrap()
            .path_style
            .is_none()
    );
}
#[test]
fn width_and_smoothness_are_validated_on_the_document_boundary() {
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    p.levels[0].layers[1].items.push(path());
    for width in [0.00390625, 0.099, 0.1, 5., 5.001, 512.] {
        if let ItemKind::Path { width: w, .. } = &mut p.levels[0].layers[1].items[0].kind {
            *w = width;
        }
        p.validate().unwrap();
    }
    for width in [0., -0.1, 512.001, f32::NAN] {
        if let ItemKind::Path { width: w, .. } = &mut p.levels[0].layers[1].items[0].kind {
            *w = width;
        }
        assert!(p.validate().is_err());
    }
    if let ItemKind::Path { width, .. } = &mut p.levels[0].layers[1].items[0].kind {
        *width = 0.5;
    }
    for smoothness in [0., 1.] {
        p.levels[0].layers[1].items[0]
            .path_style
            .as_mut()
            .unwrap()
            .smoothness = smoothness;
        p.validate().unwrap();
    }
    for smoothness in [-0.01, 1.01, f32::NAN] {
        p.levels[0].layers[1].items[0]
            .path_style
            .as_mut()
            .unwrap()
            .smoothness = smoothness;
        assert!(p.validate().is_err());
    }
}
#[test]
fn textured_paths_generate_native_gpu_raster_and_svg_exports() {
    let mut library = Library::new();
    assets::load_defaults(&mut library).unwrap();
    let mut p = Project {
        background_asset: None,
        width: 32,
        height: 24,
        ..Default::default()
    };
    let ids: Vec<_> = library
        .values()
        .filter(|a| assets::category(&a.meta) == "paths")
        .map(|a| a.meta.id.clone())
        .collect();
    assert!(!ids.is_empty());
    for n in 0..4 {
        let mut i = path();
        i.texture = Some(ids[n % ids.len()].clone());
        let width = if n == 3 { 1.5 } else { 0.5 };
        if let ItemKind::Path { width: w, .. } = &mut i.kind {
            *w = width;
        }
        i.texture_scale = library[i.texture.as_ref().unwrap()].meta.size.x
            / library[i.texture.as_ref().unwrap()].meta.size.y
            * width;
        i.path_style = Some(PathStyle {
            smoothness: if n == 0 { 0. } else { 1. },
            transition_in: if n == 2 {
                Transition::Fade
            } else {
                Transition::None
            },
            transition_out: if n == 3 {
                Transition::Shrink
            } else {
                Transition::None
            },
            blocks_light: false,
        });
        i.translate(Point::new((n % 2) as f32 * 15., (n / 2) as f32 * 11.));
        p.levels[0].layers[1].items.push(i);
    }
    p.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("textured-paths.mloom");
    formats::save(&file, &p, &library).unwrap();
    assert_eq!(formats::load(&file).unwrap().project.levels, p.levels);
    let scene = SceneRenderer::default()
        .scene(
            &p,
            &library,
            SceneOptions {
                level: 0,
                grid: false,
                lights: false,
                revision: 1,
                known_meshes: HashSet::new(),
            },
            &HashSet::new(),
        )
        .unwrap();
    assert!(scene.stats.vertices > 100);
    assert!(scene.stats.decoded_images > 0);
    let svg = render::svg(&p, &library, options()).unwrap();
    assert!(svg.contains("patternTransform=\"matrix("));
    assert!(svg.contains("linearGradient"));
    if let Some(output) = std::env::var_os("MAPLOOM_PATH_EVIDENCE") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        use base64::{Engine, engine::general_purpose::STANDARD};
        let mut thumbnails = std::collections::BTreeMap::new();
        for id in &ids {
            let image = assets::decode_surface(&library[id]).unwrap();
            let image = image::imageops::thumbnail(&image, 512, 96);
            let mut png = std::io::Cursor::new(vec![]);
            image.write_to(&mut png, image::ImageFormat::Png).unwrap();
            thumbnails.insert(
                id,
                format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(png.into_inner())
                ),
            );
        }
        let assets: Vec<_> = ids
            .iter()
            .map(
                |id| serde_json::json!({"meta":library[id].meta,"category":"paths","builtin":true}),
            )
            .collect();
        std::fs::write(output.join("ui-fixture.json"), serde_json::to_vec(&serde_json::json!({"project":p,"assets":assets,"scene":scene,"thumbnails":thumbnails})).unwrap()).unwrap();
        std::fs::write(
            output.join("scene.json"),
            serde_json::to_vec(&scene).unwrap(),
        )
        .unwrap();
        std::fs::write(output.join("project.json"), serde_json::to_vec(&p).unwrap()).unwrap();
        std::fs::write(output.join("paths.svg"), svg).unwrap();
        let (image, _) = render::rasterize(&p, &library, options()).unwrap();
        image.save(output.join("paths.png")).unwrap();
    }
}
