use base64::{engine::general_purpose::STANDARD, Engine};
use maploom::{
    assets::Library,
    document::*,
    formats,
    gpu_scene::{GpuMesh, SceneOptions, SceneRenderer},
    render,
};
use std::collections::HashSet;

fn options(lights: bool) -> SceneOptions {
    SceneOptions {
        level: 0,
        grid: false,
        lights,
        revision: 0,
        known_meshes: HashSet::new(),
    }
}
fn colors(mesh: &GpuMesh) -> Vec<[f32; 4]> {
    STANDARD
        .decode(&mesh.vertices)
        .unwrap()
        .chunks_exact(48)
        .map(|v| {
            std::array::from_fn(|i| {
                f32::from_le_bytes(v[16 + i * 4..20 + i * 4].try_into().unwrap())
            })
        })
        .collect()
}
fn fixture() -> Project {
    let mut p = Project {
        background_asset: None,
        background: [255; 4],
        ..Project::default()
    };
    p.levels[0].layers[0].items.push(Item::new(
        "Light",
        ItemKind::Light {
            position: Point::new(3., 3.),
            range: 2.,
            intensity: 1.,
            smoothing: 1.,
            color: [255, 200, 100, 255],
            shadows: false,
        },
    ));
    p
}

#[test]
fn ambient_color_defaults_to_white_roundtrips_and_supports_history() {
    let mut p = fixture();
    let mut legacy = serde_json::to_value(&p).unwrap();
    legacy.as_object_mut().unwrap().remove("ambient_light");
    assert_eq!(
        serde_json::from_value::<Project>(legacy)
            .unwrap()
            .ambient_light,
        [255; 4]
    );
    let mut history = History::default();
    history.record(p.clone());
    p.ambient_light = [40, 70, 120, 255];
    let temp = tempfile::tempdir().unwrap();
    for ext in ["json", "mloom"] {
        let file = temp.path().join(format!("ambient.{ext}"));
        formats::save(&file, &p, &Library::new()).unwrap();
        assert_eq!(formats::load(&file).unwrap().project, p);
    }
    assert!(history.undo(&mut p));
    assert_eq!(p.ambient_light, [255; 4]);
    assert!(history.redo(&mut p));
    assert_eq!(p.ambient_light, [40, 70, 120, 255]);
}

#[test]
fn gpu_tints_the_map_but_preserves_sources_and_reuses_textures() {
    let mut p = fixture();
    let mut renderer = SceneRenderer::default();
    let original = renderer
        .scene(&p, &Library::new(), options(true), &HashSet::new())
        .unwrap();
    let stamp = original
        .textures
        .iter()
        .find(|t| t.width == 512 && t.height == 512)
        .unwrap()
        .key
        .clone();
    let known = original.used_textures.iter().cloned().collect();
    p.ambient_light = [64, 128, 255, 255];
    let tinted = renderer
        .scene(&p, &Library::new(), options(true), &known)
        .unwrap();
    assert!(
        tinted.textures.is_empty(),
        "Changing ambient color must reuse textures"
    );
    let base = tinted.meshes.iter().find(|m| m.texture != stamp).unwrap();
    assert!(colors(base)
        .iter()
        .any(|c| *c == [64. / 255., 128. / 255., 1., 1.]));
    assert!(colors(base)
        .iter()
        .filter(|c| c[3] == 1.)
        .all(|c| *c == [64. / 255., 128. / 255., 1., 1.]));
    assert_eq!(
        original
            .meshes
            .iter()
            .find(|m| m.texture == stamp)
            .unwrap()
            .vertices,
        tinted
            .meshes
            .iter()
            .find(|m| m.texture == stamp)
            .unwrap()
            .vertices
    );
    p.ambient_light = [0, 0, 0, 255];
    let dark = renderer
        .scene(&p, &Library::new(), options(true), &known)
        .unwrap();
    assert!(
        colors(dark.meshes.iter().find(|m| m.texture != stamp).unwrap())
            .iter()
            .all(|c| c[..3] == [0.; 3])
    );
    let unlit = renderer
        .scene(&p, &Library::new(), options(false), &known)
        .unwrap();
    assert!(unlit.meshes.iter().all(|m| m.texture != stamp));
    assert!(colors(&unlit.meshes[0])
        .iter()
        .filter(|c| c[3] == 1.)
        .all(|c| *c == [1.; 4]));
}

#[test]
fn native_scene_and_exports_apply_ambient_only_to_the_map() {
    let mut p = fixture();
    p.ambient_light = [64, 128, 255, 255];
    let ctx = egui::Context::default();
    let output = ctx.run(Default::default(), |ctx| {
        render::draw_scene(
            &ctx.layer_painter(egui::LayerId::background()),
            &p,
            0,
            &Default::default(),
            &render::View {
                origin: egui::Pos2::ZERO,
                scale: 32.,
                grid: false,
                lights: true,
            },
        )
    });
    let base = output
        .shapes
        .iter()
        .find_map(|s| {
            if let egui::Shape::Rect(rect) = &s.shape {
                Some(rect.fill.to_array())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(base, p.ambient_light);
    let opts = render::ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    let svg = render::svg(&p, &Library::new(), opts).unwrap();
    let image_start = svg.find("<image x=\"1\" y=\"1\"").unwrap();
    assert!(
        svg[..image_start].contains("</g><g opacity="),
        "Sources follow the ambient filter group"
    );
    assert!(svg.contains("0.2509804 0 0 0 0 0 0.5019608"));
    assert!(!render::svg(
        &p,
        &Library::new(),
        render::ExportOptions {
            lights: false,
            ..opts
        }
    )
    .unwrap()
    .contains("feColorMatrix"));
    assert_eq!(
        formats::dd2vtt(&p, 0, 32, b"PNG", &Library::new())["environment"]["ambient_light"],
        "ff4080ff"
    );
}

#[test]
fn ambient_light_multiplies_red_mask_output_without_changing_the_selected_tint() {
    use maploom::assets::{Asset, Source};
    use std::{io::Cursor, sync::Arc};
    let mut png = Cursor::new(vec![]);
    image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 30, 20, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let meta = AssetMeta {
        id: "mask".into(),
        name: "Mask".into(),
        path: "mask.png".into(),
        tags: vec![],
        size: Point::new(2., 2.),
        pivot: Point::new(0.5, 0.5),
        sha256: String::new(),
        color_mode: ObjectColorMode::RedMask,
    };
    let library = Library::from([(
        "mask".into(),
        Asset {
            meta,
            source: Source::Bytes(Arc::new(png.into_inner())),
        },
    )]);
    let mut p = Project {
        background_asset: None,
        ambient_light: [64, 128, 192, 255],
        ..Project::default()
    };
    let mut item = Item::new(
        "Masked object",
        ItemKind::Object {
            asset: "mask".into(),
            position: Point::new(4., 4.),
            size: Point::new(2., 2.),
            pivot: Point::new(0.5, 0.5),
            rotation: 0.,
            flip_x: false,
            flip_y: false,
        },
    );
    item.tint = [100, 200, 50, 128];
    p.levels[0].layers[0].items.push(item);
    let scene = SceneRenderer::default()
        .scene(&p, &library, options(true), &HashSet::new())
        .unwrap();
    let masked = scene.meshes.iter().find(|m| m.mode == 1).unwrap();
    assert!(colors(masked)
        .iter()
        .all(|c| *c == [64. / 255., 128. / 255., 192. / 255., 1.]));
    for vertex in STANDARD.decode(&masked.vertices).unwrap().chunks_exact(48) {
        let tint: [f32; 4] = std::array::from_fn(|i| {
            f32::from_le_bytes(vertex[32 + i * 4..36 + i * 4].try_into().unwrap())
        });
        assert!((tint[0] * 255. - 100.).abs() <= 1.);
        assert!((tint[1] * 255. - 200.).abs() <= 1.);
        assert!((tint[2] * 255. - 50.).abs() <= 1.);
        assert!((tint[3] * 255. - 128.).abs() <= 1.);
    }
}

#[test]
#[ignore = "Requires a hardware GPU; run explicitly for native export verification"]
fn raster_export_bakes_the_selected_environment_color() {
    let mut p = fixture();
    p.width = 8;
    p.height = 8;
    p.background = [180, 180, 180, 255];
    p.ambient_light = [64, 64, 128, 255];
    let opts = render::ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("environment.dd2vtt");
    let gpu = render::export(&path, &p, &Library::new(), opts).unwrap();
    let vtt: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(vtt["environment"]["baked_lighting"], true);
    assert_eq!(vtt["environment"]["ambient_light"], "ff404080");
    let image = image::load_from_memory(&STANDARD.decode(vtt["image"].as_str().unwrap()).unwrap())
        .unwrap()
        .to_rgba8();
    let corner = image.get_pixel(32, 32).0;
    for (actual, expected) in corner.into_iter().zip([45u8, 45, 90, 255]) {
        assert!(actual.abs_diff(expected) <= 1, "Export pixel {corner:?}");
    }
    assert!(
        image.get_pixel(96, 96)[0] > corner[0],
        "Local light stays visible"
    );
    println!("{gpu}; ambient export pixel: {corner:?}");
}
