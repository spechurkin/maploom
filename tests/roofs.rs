use maploom::{
    assets::{self, Library},
    document::*,
    formats, geometry,
    gpu_scene::{SceneOptions, SceneRenderer},
    render,
    roofs::{self, RoofType},
};
use std::collections::HashSet;
fn points() -> Vec<Point> {
    vec![Point::new(2., 4.), Point::new(12., 4.)]
}
fn item(kind: RoofType, points: Vec<Point>, width: f32) -> Item {
    let id = |part| Some(format!("res://textures/roofs/flat_clay_red/{part}.png"));
    let mut i = Item::new(
        "Roof",
        ItemKind::Roof {
            points,
            width,
            roof_type: kind,
            ridge: id("ridge"),
            edge: id("edge"),
            hip: id("hip"),
            shade: true,
            sun_direction: 135.,
            contrast: 0.3,
        },
    );
    i.texture = id("tiles");
    i.texture_scale = 4.;
    i
}
fn options() -> render::ExportOptions {
    render::ExportOptions {
        level: 0,
        pixels_per_grid: 48,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    }
}
#[test]
fn types_partition_their_footprints_and_shade_each_slope() {
    for kind in [RoofType::Gable, RoofType::Hip, RoofType::Dormer] {
        let g = roofs::build(&points(), 4., kind);
        assert!(roofs::valid(&points(), 4., kind));
        let area: f32 = g
            .facets
            .iter()
            .map(|f| {
                render::triangulate(&f.points)
                    .iter()
                    .map(|t| {
                        geometry::area(&[f.points[t[0]], f.points[t[1]], f.points[t[2]]]).abs()
                    })
                    .sum::<f32>()
            })
            .sum();
        assert!((area - geometry::area(&g.outline).abs()).abs() < 0.001);
        assert!(g
            .facets
            .iter()
            .any(|f| f.brightness(true, 135., 0.3) < 0.85));
        assert!(g
            .facets
            .iter()
            .all(|f| f.brightness(false, 135., 0.3) == 1.));
    }
    assert_eq!(roofs::build(&points(), 4., RoofType::Hip).hips.len(), 4);
    // A square hip roof reduces the ridge to a single apex, without losing triangles.
    let g = roofs::build(&[Point::new(0., 0.), Point::new(4., 0.)], 4., RoofType::Hip);
    let area: f32 = g
        .facets
        .iter()
        .map(|f| geometry::area(&f.points).abs())
        .sum();
    assert!((area - 16.).abs() < 0.001);
}
#[test]
fn manual_bent_ridge_has_mitered_eaves_and_correct_selection() {
    let ps = vec![
        Point::new(2., 4.),
        Point::new(10., 2.),
        Point::new(16., 16.),
        Point::new(24., 12.),
        Point::new(22., 5.),
    ];
    assert!(roofs::valid(&ps, 3., RoofType::Gable));
    let g = roofs::build(&ps, 3., RoofType::Gable);
    assert_eq!(g.ridge, ps);
    assert_eq!(g.facets.len(), 8);
    let area: f32 = g
        .facets
        .iter()
        .map(|f| geometry::area(&f.points).abs())
        .sum();
    assert!((area - geometry::area(&g.outline).abs()).abs() < 0.001);
    let mut i = item(RoofType::Gable, points(), 4.);
    assert!(render::hit(&i, Point::new(3., 5.9), 0.));
    assert!(!render::hit(&i, Point::new(3., 6.1), 0.));
    assert!(render::intersects_box(
        &i,
        Point::new(3., 5.8),
        Point::new(4., 6.2)
    ));
    i.translate(Point::new(5., 7.));
    assert!(render::hit(&i, Point::new(8., 12.9), 0.));
    assert!(!roofs::valid(
        &[Point::new(0., 0.), Point::new(0., 0.)],
        4.,
        RoofType::Gable
    ));
    assert!(!roofs::valid(
        &[Point::new(0., 0.), Point::new(4., 0.), Point::new(0., 0.)],
        4.,
        RoofType::Gable
    ));
}
#[test]
fn roofs_roundtrip_all_four_textures_and_generate_gpu_and_svg() {
    let mut library = Library::new();
    assets::load_defaults(&mut library).unwrap();
    let mut p = Project {
        background_asset: None,
        width: 32,
        height: 24,
        ..Default::default()
    };
    for (n, kind) in [RoofType::Gable, RoofType::Hip, RoofType::Dormer]
        .into_iter()
        .enumerate()
    {
        let mut i = item(kind, points(), 4.);
        i.translate(Point::new(0., n as f32 * 6.));
        p.levels[0].layers[1].items.push(i);
    }
    p.levels[0].layers[1].items.push(item(
        RoofType::Gable,
        vec![
            Point::new(17., 3.),
            Point::new(26., 5.),
            Point::new(22., 14.),
            Point::new(29., 18.),
        ],
        3.,
    ));
    p.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("roofs.mloom");
    formats::save(&path, &p, &library).unwrap();
    let loaded = formats::load(&path).unwrap();
    assert_eq!(loaded.project.levels, p.levels);
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
    assert!(scene.meshes.len() > 3);
    assert_eq!(scene.stats.decoded_images, 4);
    let svg = render::svg(&p, &library, options()).unwrap();
    assert!(svg.contains("patternTransform="));
    assert!(svg.contains("<polygon"));
    let mut missing = library.clone();
    missing.remove("res://textures/roofs/flat_clay_red/hip.png");
    assert!(assets::require_assets(&p, 0, &missing).is_err());
    if let Some(output) = std::env::var_os("MAPLOOM_ROOF_EVIDENCE") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(
            output.join("scene.json"),
            serde_json::to_vec(&scene).unwrap(),
        )
        .unwrap();
        std::fs::write(output.join("project.json"), serde_json::to_vec(&p).unwrap()).unwrap();
        std::fs::write(output.join("roofs.svg"), svg).unwrap();
        let (image, _) = render::rasterize(&p, &library, options()).unwrap();
        image.save(output.join("roofs.png")).unwrap();
    }
}
