use maploom::{
    assets::{self, Library},
    caves,
    document::*,
    editing, formats, geometry,
    gpu_scene::{SceneOptions, SceneRenderer},
    openings::Openings,
    render,
};
use std::collections::HashSet;
fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ]
}
fn cave(style: &str) -> Item {
    let mut i = Item::new(
        "Cave",
        ItemKind::TerrainRegion {
            rings: vec![rect(2., 2., 12., 10.)],
            color: [255; 4],
        },
    );
    i.cave = Some(CaveStyle {
        colorable: style == "colorable",
        ground_color: [127, 126, 113, 255],
        wall_color: [127, 126, 113, 255],
        blasts: vec![],
    });
    i.texture = Some(format!("res://textures/caves/{style}/floor.png"));
    i.border = Some(format!("res://textures/caves/{style}/wall.png"));
    i.texture_scale = 8.;
    i
}
fn room() -> Item {
    Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![rect(7., 1., 5., 8.)],
            floor: Some("res://textures/tilesets/simple/tileset_wood.png".into()),
            wall: Some("res://textures/walls/stone.png".into()),
            width: 0.25,
        },
    )
}
fn length(lines: Vec<[Point; 2]>) -> f32 {
    lines.iter().map(|l| l[0].distance(l[1])).sum()
}

#[test]
fn blast_removes_floor_and_existing_rock_without_adding_a_new_boundary() {
    let mut i = cave("rocky");
    i.cave.as_mut().unwrap().blasts = vec![rect(6., 0., 3., 8.), rect(10., 10., 2., 4.)];
    let floor = caves::floor(&i, &[]);
    assert!(!geometry::inside(&floor, Point::new(7., 5.)));
    assert!(geometry::inside(&floor, Point::new(4., 5.)));
    let boundary = caves::boundary(&i, &[]);
    assert!((length(boundary.clone()) - 39.).abs() < 1e-4);
    assert!(boundary.iter().all(|l| !geometry::inside(
        &i.cave.as_ref().unwrap().blasts,
        Point::new((l[0].x + l[1].x) / 2., (l[0].y + l[1].y) / 2.)
    )));
    assert!(!render::hit(&i, Point::new(7., 5.), 0.));
    assert!(i.wall_segments().is_empty());
    assert!(maploom::portals::attach(&i, Point::new(2., 5.), 1.).is_none());
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    p.levels[0].layers[1].items.push(i);
    assert!(editing::attach_portal(&p, 0, Point::new(2., 5.), 1.).is_err());
    assert!(
        (formats::line_of_sight(&p.levels[0])
            .iter()
            .map(|l| l[0].distance(l[1]))
            .sum::<f32>()
            - 39.)
            .abs()
            < 1e-4
    );
}

#[test]
fn rooms_win_in_both_draw_orders_across_layers_and_internal_boundaries_disappear() {
    let c = cave("rocky");
    let r = room();
    for reverse in [false, true] {
        let mut p = Project {
            background_asset: None,
            ..Default::default()
        };
        p.levels[0].layers[if reverse { 2 } else { 0 }]
            .items
            .push(c.clone());
        p.levels[0].layers[1].items.push(r.clone());
        let level = &p.levels[0];
        let rooms = caves::rooms(level);
        let floor = caves::floor(&c, &rooms);
        assert!(!geometry::inside(&floor, Point::new(9., 5.)));
        assert!(geometry::inside(&floor, Point::new(4., 5.)));
        assert!((length(caves::boundary(&c, &rooms)) - 39.).abs() < 1e-4);
        let openings = Openings::new(level).for_wall(&r.id);
        assert_eq!(
            openings.spans([Point::new(7., 1.), Point::new(12., 1.)]),
            vec![(0., 1.)]
        );
        assert!(openings
            .spans([Point::new(12., 9.), Point::new(7., 9.)])
            .is_empty());
        let spans = openings.spans([Point::new(12., 1.), Point::new(12., 9.)]);
        assert_eq!(spans, vec![(0., 0.125)]);
        assert!(editing::attach_portal(&p, 0, Point::new(9., 9.), 1.).is_err());
        assert!(editing::attach_portal(&p, 0, Point::new(9., 1.), 1.).is_ok());
        // Hiding or moving the room restores the original cave edge automatically.
        let mut hidden = level.clone();
        hidden.layers[1].visible = false;
        assert_eq!(length(caves::boundary(&c, &caves::rooms(&hidden))), 44.);
    }
}

#[test]
fn enclosed_blasts_and_room_holes_remain_open_and_coincident_room_edges_win() {
    let mut c = cave("colorable");
    c.cave.as_mut().unwrap().blasts = vec![rect(5., 5., 2., 2.)];
    assert_eq!(length(caves::boundary(&c, &[])), 44.);
    assert!(!geometry::inside(&caves::footprint(&c), Point::new(6., 6.)));
    let rings = vec![rect(2., 2., 5., 5.), rect(3., 3., 2., 2.)];
    assert!(!geometry::inside(
        &caves::floor(&c, &rings),
        Point::new(2.5, 2.5)
    ));
    assert!(geometry::inside(
        &caves::floor(&c, &rings),
        Point::new(4., 4.)
    ));
    assert_eq!(
        caves::outside([Point::new(2., 2.), Point::new(7., 2.)], &rings, false),
        vec![(0., 1.)]
    );
    assert!(caves::outside([Point::new(2., 2.), Point::new(7., 2.)], &rings, true).is_empty());
}

#[test]
fn caves_embed_both_styles_roundtrip_colors_and_blasts_and_render_in_gpu_svg() {
    let mut library = Library::new();
    assets::load_defaults(&mut library).unwrap();
    for style in ["colorable", "rocky"] {
        for part in ["floor", "wall"] {
            let asset = &library[&format!("res://textures/caves/{style}/{part}.png")];
            assert_eq!(assets::category(&asset.meta), "caves");
            assets::verified_bytes(asset).unwrap();
            assert_eq!(
                asset.meta.color_mode,
                if style == "colorable" {
                    ObjectColorMode::Tint
                } else {
                    ObjectColorMode::None
                }
            );
        }
    }
    let mut p = Project {
        background_asset: None,
        width: 32,
        height: 24,
        background: [90, 70, 50, 255],
        ..Default::default()
    };
    let mut c = cave("colorable");
    c.cave.as_mut().unwrap().blasts = vec![rect(6., 0., 3., 8.)];
    let mut rocky = cave("rocky");
    rocky.translate(Point::new(15., 0.));
    p.levels[0].layers[1].items.extend([c, rocky]);
    let mut r = room();
    if let ItemKind::Room { floor, .. } = &mut r.kind {
        *floor = Some("res://textures/tilesets/simple/tileset_wood_interlaced.png".into());
    }
    r.floor_tint = [174, 134, 86, 255];
    r.wall_tint = [127, 126, 113, 255];
    p.levels[0].layers[1].items.push(r);
    p.validate().unwrap();
    let dir = tempfile::tempdir().unwrap();
    for ext in ["mloom", "json"] {
        let path = dir.path().join(format!("caves.{ext}"));
        formats::save(&path, &p, &library).unwrap();
        let loaded = formats::load(&path).unwrap();
        assert_eq!(loaded.project.levels, p.levels);
        assert_eq!(loaded.library.len(), 6);
    }
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
    assert_eq!(scene.stats.decoded_images, 6);
    assert!(scene.meshes.len() > 4);
    let options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 64,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    let svg = render::svg(&p, &library, options).unwrap();
    assert!(svg.contains("cavecut"));
    let mut moved = p.levels[0].layers[1].items[0].clone();
    moved.translate(Point::new(3., 4.));
    assert!(!render::hit(&moved, Point::new(10., 9.), 0.));
    let (image, _) = render::rasterize(&p, &library, options).unwrap();
    assert_eq!(image.get_pixel(6 * 64 + 32, 5 * 64).0, p.background);
    assert_eq!(image.get_pixel(6 * 64 + 32, 2 * 64).0, p.background);
    // Square floor corners must not peek through the rounded, translucent rocks.
    for x in [2, 14, 17, 29] {
        for y in [2, 12] {
            let pixel = image.get_pixel(x * 64, y * 64).0;
            assert!(
                pixel
                    .iter()
                    .zip(p.background)
                    .all(|(a, b)| a.abs_diff(b) < 12),
                "Exposed floor corner at {x},{y}: {pixel:?}"
            );
        }
    }
    if let Some(output) = std::env::var_os("MAPLOOM_CAVE_EVIDENCE") {
        let out = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("caves.svg"), svg).unwrap();
        std::fs::write(out.join("scene.json"), serde_json::to_vec(&scene).unwrap()).unwrap();
        std::fs::write(out.join("project.json"), serde_json::to_vec(&p).unwrap()).unwrap();
        image.save(out.join("caves.png")).unwrap();
    }
}

#[test]
fn curved_cave_edges_render_both_materials() {
    let mut library = Library::new();
    assets::load_defaults(&mut library).unwrap();
    let mut p = Project {
        background_asset: None,
        width: 28,
        height: 16,
        background: [148, 173, 105, 255],
        ..Default::default()
    };
    for (n, style) in ["colorable", "rocky"].into_iter().enumerate() {
        let mut c = cave(style);
        c.kind = ItemKind::TerrainRegion {
            rings: geometry::terrain_contours(&[
                (Point::new(6. + n as f32 * 14., 6.), 3.8),
                (Point::new(8. + n as f32 * 14., 10.), 3.5),
            ]),
            color: [255; 4],
        };
        p.levels[0].layers[1].items.push(c);
    }
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
    assert_eq!(scene.stats.decoded_images, 4);
    if let Some(output) = std::env::var_os("MAPLOOM_CAVE_CURVE_EVIDENCE") {
        let out = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&out).unwrap();
        let options = render::ExportOptions {
            level: 0,
            pixels_per_grid: 96,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        };
        let (image, _) = render::rasterize(&p, &library, options).unwrap();
        image.save(out.join("curved.png")).unwrap();
        std::fs::write(
            out.join("curved.svg"),
            render::svg(&p, &library, options).unwrap(),
        )
            .unwrap();
        std::fs::write(out.join("scene.json"), serde_json::to_vec(&scene).unwrap()).unwrap();
    }
}

#[test]
fn rock_ribbons_do_not_fold_at_brush_steps_and_keep_native_texture_scale() {
    let rings =
        geometry::terrain_contours(&[(Point::new(6., 6.), 3.8), (Point::new(8., 10.), 3.5)]);
    let before = rings.clone();
    for width in [135. / 256., 270. / 256.] {
        let scale = width * 1024. / 135.;
        for ring in &rings {
            for reverse in [false, true] {
                let mut ring = ring.clone();
                if reverse {
                    ring.reverse();
                }
                let ribbon = caves::wall_ribbon(&ring, width, scale);
                assert!(!ribbon.is_empty());
                assert_eq!(ribbon[0].position, ribbon[ribbon.len() - 2].position);
                assert_eq!(ribbon[1].position, ribbon[ribbon.len() - 1].position);
                let mut length = 0.;
                for rows in ribbon.chunks_exact(2).collect::<Vec<_>>().windows(2) {
                    let [a, b, c, d] = [rows[0][0], rows[1][0], rows[1][1], rows[0][1]];
                    let cross = |a: Point, b: Point, c: Point| {
                        (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
                    };
                    assert!(
                        cross(a.position, b.position, c.position) < 0.,
                        "Folded inner rock edge"
                    );
                    assert!(
                        cross(a.position, c.position, d.position) < 0.,
                        "Folded outer rock edge"
                    );
                    let center =
                        |a: Point, b: Point| Point::new((a.x + b.x) / 2., (a.y + b.y) / 2.);
                    let step =
                        center(a.position, d.position).distance(center(b.position, c.position));
                    length += step;
                    assert!(((b.uv.x - a.uv.x) * scale - step).abs() < 1e-4);
                    assert!((a.position.distance(d.position) - width).abs() < 1e-5);
                }
                assert!((ribbon.last().unwrap().uv.x - length / scale).abs() < 1e-4);
            }
        }
    }
    assert_eq!(rings, before);
    assert!(caves::wall_ribbon(&[], 1., 4.).is_empty());
    assert!(caves::wall_ribbon(&[Point::new(1., 1.); 4], 1., 4.).is_empty());
}

#[test]
fn visible_floor_follows_rounded_rocks_at_corners_and_holes_without_changing_saved_geometry() {
    for (style, width) in [("colorable", 135. / 256.), ("rocky", 270. / 256.)] {
        let mut c = cave(style);
        if let ItemKind::TerrainRegion { rings, .. } = &mut c.kind {
            rings.push(rect(5., 5., 3., 3.).into_iter().rev().collect());
        }
        c.cave.as_mut().unwrap().blasts = vec![rect(10., 1., 1., 4.)];
        let saved = c.clone();
        let rooms = vec![rect(10., 8., 3., 5.)];
        let floor = caves::render_floor(&c, &rooms, width);
        // The previous square floor protruded outside the rounded artwork here.
        assert!(!geometry::inside(&floor, Point::new(2.01, 2.01)));
        assert!(!geometry::inside(&floor, Point::new(13.99, 11.99)));
        assert!(geometry::inside(&floor, Point::new(3., 3.)));
        assert!(!geometry::inside(&floor, Point::new(6., 6.)));
        assert!(!geometry::inside(&floor, Point::new(10.5, 3.)));
        assert!(!geometry::inside(&floor, Point::new(11., 9.)));
        assert_eq!(c, saved);
        assert!(geometry::inside(
            &caves::floor(&c, &[]),
            Point::new(2.01, 2.01)
        ));
    }
}
