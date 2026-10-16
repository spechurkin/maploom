use base64::{engine::general_purpose::STANDARD, Engine};
use maploom::{
    assets::{Asset, Library, Source},
    document::*,
    gpu_scene::{GpuScene, SceneOptions, SceneRenderer},
    light::Blockers,
    render,
};
use std::{collections::HashSet, sync::Arc};

fn point(x: f32, y: f32) -> Point {
    Point::new(x, y)
}
fn wall(a: Point, b: Point) -> Item {
    Item::new(
        "Wall",
        ItemKind::Wall {
            points: vec![a, b],
            width: 0.2,
            color: [255; 4],
            blocks_sight: true,
        },
    )
        .with_texture(Some("wall"))
}
fn portal(kind: PortalType) -> Item {
    let mut item = Item::new(
        "Renamed opening",
        ItemKind::Portal {
            bounds: [point(6., 4.), point(6., 6.)],
            closed: true,
            color: [255; 4],
        },
    )
        .with_texture(Some("portal"));
    item.portal_type = Some(kind);
    item
}
fn fixture() -> Project {
    let mut p = Project {
        background_asset: None,
        use_default_assets: false,
        background: [20, 20, 20, 255],
        ..Default::default()
    };
    p.levels[0].layers[0].items.push(Item::new(
        "Light",
        ItemKind::Light {
            position: point(4., 5.),
            range: 8.,
            intensity: 2.,
            color: [255, 180, 70, 255],
            shadows: true,
            smoothing: 1.,
        },
    ));
    p.levels[0].layers[1]
        .items
        .push(wall(point(6., -100.), point(6., 100.)));
    p
}
fn visible(p: &Project, library: &Library, origin: Point, target: Point) -> bool {
    Blockers::new(p, 0, library)
        .visibility(origin, 12.)
        .is_none_or(|polygon| render::contains_polygon(&polygon, target))
}

fn object() -> Item {
    Item::new(
        "Crate",
        ItemKind::Object {
            asset: "wall".into(),
            position: point(6., 5.),
            size: point(1., 2.),
            rotation: 0.,
            pivot: point(0.5, 0.5),
            flip_x: false,
            flip_y: false,
        },
    )
}

fn padded_library() -> Library {
    let mut library = library();
    let mut image = image::RgbaImage::new(16, 16);
    for y in 4..12 {
        for x in 4..12 {
            image.put_pixel(x, y, image::Rgba([60, 60, 60, 255]));
        }
    }
    let mut png = std::io::Cursor::new(vec![]);
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let asset = library.get_mut("wall").unwrap();
    asset.meta.sha256 = maploom::assets::hash(png.get_ref());
    asset.source = Source::Bytes(Arc::new(png.into_inner()));
    library
}

#[test]
fn object_shadows_start_at_the_artwork_in_gpu_raster_svg_and_vtt() {
    let mut p = fixture();
    p.width = 12;
    p.height = 10;
    let mut blocker = object();
    blocker.blocks_light = true;
    p.levels[0].layers[1].items = vec![blocker];
    let mut library = padded_library();
    let origin = point(4., 5.);
    let margin = point(5.6, 5.);
    let beside = point(8., 7.);
    let behind = point(8., 5.);
    assert!(visible(&p, &library, origin, margin));
    assert!(visible(&p, &library, origin, beside));
    assert!(!visible(&p, &library, origin, behind));
    let mut renderer = SceneRenderer::default();
    let mut make_scene = |p: &Project, library: &Library, revision| {
        renderer
            .scene(
                p,
                library,
                SceneOptions {
                    level: 0,
                    grid: false,
                    lights: true,
                    revision,
                    known_meshes: HashSet::new(),
                },
                &HashSet::new(),
            )
            .unwrap()
    };
    let scene = make_scene(&p, &library, 0);
    assert!(light_covers(&scene, margin));
    assert!(light_covers(&scene, beside));
    assert!(!light_covers(&scene, behind));
    let cached = make_scene(&p, &library, 1);
    assert_eq!(cached.stats.decoded_images, 0);
    assert!(light_covers(&cached, beside));

    let options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 64,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    let (raster, _) = render::rasterize(&p, &library, options).unwrap();
    assert!(raster.get_pixel(8 * 64, 7 * 64)[0] > p.background[0] + 5);
    assert_eq!(raster.get_pixel(8 * 64, 5 * 64).0, p.background);
    let vtt = maploom::formats::dd2vtt(&p, 0, 64, &[], &library);
    let edges: Vec<Vec<Point>> = serde_json::from_value(vtt["line_of_sight"].clone()).unwrap();
    assert_eq!(edges.len(), 4);
    assert!(edges
        .iter()
        .flatten()
        .all(|v| (5.75..=6.25).contains(&v.x) && (4.5..=5.5).contains(&v.y)));
    let svg = render::svg(&p, &library, options).unwrap();
    let polygon = Blockers::new(&p, 0, &library)
        .visibility(origin, 8.)
        .unwrap();
    let coordinates = polygon
        .iter()
        .map(|v| format!("{},{}", v.x, v.y))
        .collect::<Vec<_>>()
        .join(" ");
    assert!(svg.contains(&format!("<polygon points=\"{coordinates}\"")));

    // Replacing an image under the same asset ID must invalidate its alpha
    // boundary. Fully transparent artwork must cast no shadow at all.
    for (revision, alpha, blocked) in [(2, 255, true), (3, 0, false)] {
        let mut png = std::io::Cursor::new(vec![]);
        image::RgbaImage::from_pixel(16, 16, image::Rgba([60, 60, 60, alpha]))
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let asset = library.get_mut("wall").unwrap();
        asset.meta.sha256 = maploom::assets::hash(png.get_ref());
        asset.source = Source::Bytes(Arc::new(png.into_inner()));
        let updated = make_scene(&p, &library, revision);
        assert_eq!(light_covers(&updated, beside), !blocked);
        assert_eq!(visible(&p, &library, origin, beside), !blocked);
    }
}

#[test]
fn alpha_blockers_follow_asymmetric_artwork_pivot_rotation_and_both_flips() {
    let mut p = fixture();
    let mut blocker = object();
    blocker.blocks_light = true;
    blocker.kind = ItemKind::Object {
        asset: "wall".into(),
        position: point(6., 5.),
        size: point(4., 2.),
        pivot: point(0.25, 0.75),
        rotation: 0.,
        flip_x: false,
        flip_y: false,
    };
    p.levels[0].layers[1].items = vec![blocker];
    let mut library = padded_library();
    let mut image = image::RgbaImage::new(16, 16);
    for y in 2..6 {
        for x in 2..6 {
            image.put_pixel(x, y, image::Rgba([60, 60, 60, 255]));
        }
    }
    let mut png = std::io::Cursor::new(vec![]);
    image.write_to(&mut png, image::ImageFormat::Png).unwrap();
    let asset = library.get_mut("wall").unwrap();
    asset.meta.sha256 = maploom::assets::hash(png.get_ref());
    asset.source = Source::Bytes(Arc::new(png.into_inner()));
    // Local occupied rectangle: x=.125..375, y=.125..375.
    for (rotation, flip_x, flip_y, expected) in [
        (0., false, false, [5.5, 6.5, 3.75, 4.25]),
        (0., true, false, [7.5, 8.5, 3.75, 4.25]),
        (0., false, true, [5.5, 6.5, 4.75, 5.25]),
        (
            std::f32::consts::FRAC_PI_2,
            true,
            true,
            [5.75, 6.25, 6.5, 7.5],
        ),
    ] {
        if let ItemKind::Object {
            rotation: r,
            flip_x: x,
            flip_y: y,
            ..
        } = &mut p.levels[0].layers[1].items[0].kind
        {
            *r = rotation;
            *x = flip_x;
            *y = flip_y;
        }
        let edges = maploom::formats::line_of_sight_with_library(&p.levels[0], &library);
        assert_eq!(edges.len(), 4);
        let xs: Vec<_> = edges.iter().flatten().map(|v| v.x).collect();
        let ys: Vec<_> = edges.iter().flatten().map(|v| v.y).collect();
        for (actual, wanted) in [
            (
                xs.iter().copied().fold(f32::INFINITY, f32::min),
                expected[0],
            ),
            (
                xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                expected[1],
            ),
            (
                ys.iter().copied().fold(f32::INFINITY, f32::min),
                expected[2],
            ),
            (
                ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
                expected[3],
            ),
        ] {
            assert!((actual - wanted).abs() < 1e-5, "{actual} != {wanted}");
        }
    }
}

#[test]
fn object_light_blocking_is_optional_persistent_and_follows_transforms_and_visibility() {
    let mut p = fixture();
    p.levels[0].layers[1].items = vec![object()];
    let source = point(4., 5.);
    let target = point(8., 5.);
    assert!(visible(&p, &Library::new(), source, target));
    let legacy = serde_json::to_value(&p.levels[0].layers[1].items[0]).unwrap();
    assert!(legacy.get("blocks_light").is_none());
    assert!(!serde_json::from_value::<Item>(legacy).unwrap().blocks_light);
    p.levels[0].layers[1].items[0].blocks_light = true;
    let dir = tempfile::tempdir().unwrap();
    for ext in ["mloom", "json"] {
        let path = dir.path().join(format!("objects.{ext}"));
        maploom::formats::save(&path, &p, &library()).unwrap();
        let loaded = maploom::formats::load(&path).unwrap();
        assert!(loaded.project.levels[0].layers[1].items[0].blocks_light);
        assert!(!visible(&loaded.project, &loaded.library, source, target));
    }
    assert!(!visible(&p, &Library::new(), source, target));
    assert!(visible(&p, &Library::new(), source, point(8., 9.)));
    // A noncentral pivot and a quarter turn place the footprint at x=6..8, y=4..5.
    if let ItemKind::Object {
        position,
        size,
        pivot,
        rotation,
        flip_x,
        flip_y,
        ..
    } = &mut p.levels[0].layers[1].items[0].kind
    {
        *position = point(8., 4.);
        *size = point(1., 2.);
        *pivot = point(0., 0.);
        *rotation = std::f32::consts::FRAC_PI_2;
        *flip_x = true;
        *flip_y = true;
    }
    assert!(!visible(
        &p,
        &Library::new(),
        point(4., 4.5),
        point(9., 4.5)
    ));
    assert!(visible(&p, &Library::new(), point(4., 5.5), point(9., 5.5)));
    p.levels[0].layers[1].items[0].translate(point(0., 5.));
    assert!(visible(&p, &Library::new(), point(4., 4.5), point(9., 4.5)));
    p.levels[0].layers[1].items[0] = object();
    p.levels[0].layers[1].items[0].blocks_light = true;
    p.levels[0].layers[1].visible = false;
    assert!(visible(&p, &Library::new(), source, target));
}

#[test]
fn cave_walls_block_light_but_blasts_and_hidden_caves_leave_openings() {
    let mut p = fixture();
    let mut cave = Item::new(
        "Cave",
        ItemKind::TerrainRegion {
            rings: vec![vec![
                point(2., 2.),
                point(7., 2.),
                point(7., 8.),
                point(2., 8.),
            ]],
            color: [255; 4],
        },
    );
    cave.cave = Some(CaveStyle {
        colorable: true,
        ground_color: [255; 4],
        wall_color: [255; 4],
        blasts: vec![],
    });
    p.levels[0].layers[1].items = vec![cave];
    let source = point(4., 5.);
    assert!(visible(&p, &Library::new(), source, point(6., 5.)));
    assert!(!visible(&p, &Library::new(), source, point(9., 5.)));
    assert!(!visible(&p, &Library::new(), point(9., 5.), source));
    p.levels[0].layers[1].items[0].cave.as_mut().unwrap().blasts = vec![vec![
        point(6., 4.),
        point(8., 4.),
        point(8., 6.),
        point(6., 6.),
    ]];
    assert!(visible(&p, &Library::new(), source, point(9., 5.)));
    assert!(!visible(&p, &Library::new(), source, point(9., 8.)));
    p.levels[0].layers[1].visible = false;
    assert!(visible(&p, &Library::new(), source, point(9., 8.)));
}

#[test]
fn objects_and_caves_clip_gpu_svg_raster_and_vtt_consistently() {
    let mut p = fixture();
    p.width = 12;
    p.height = 10;
    let mut renderer = SceneRenderer::default();
    let options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 24,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    for is_cave in [false, true] {
        let mut blocker = if is_cave {
            let mut cave = Item::new(
                "Cave",
                ItemKind::TerrainRegion {
                    rings: vec![vec![
                        point(2., 2.),
                        point(6., 2.),
                        point(6., 8.),
                        point(2., 8.),
                    ]],
                    color: [255; 4],
                },
            );
            cave.cave = Some(CaveStyle {
                colorable: true,
                ground_color: [255; 4],
                wall_color: [255; 4],
                blasts: vec![],
            });
            cave
        } else {
            object()
        };
        blocker.blocks_light = true;
        p.levels[0].layers[1].items = vec![blocker];
        assert!(!light_covers(
            &scene(&p, &mut renderer, if is_cave { 2 } else { 0 }),
            point(8., 5.)
        ));
        assert!(render::svg(&p, &library(), options)
            .unwrap()
            .contains("clipPathUnits=\"userSpaceOnUse\"><polygon"));
        let (blocked, _) = render::rasterize(&p, &library(), options).unwrap();
        assert_eq!(blocked.get_pixel(8 * 24, 5 * 24).0, p.background);
        let vtt = maploom::formats::dd2vtt(&p, 0, 24, &[], &library());
        assert_eq!(vtt["line_of_sight"].as_array().unwrap().len(), 4);
        if is_cave {
            p.levels[0].layers[1].visible = false;
        } else {
            p.levels[0].layers[1].items[0].blocks_light = false;
        }
        assert!(light_covers(
            &scene(&p, &mut renderer, if is_cave { 3 } else { 1 }),
            point(8., 5.)
        ));
        let (unblocked, _) = render::rasterize(&p, &library(), options).unwrap();
        assert!(unblocked.get_pixel(8 * 24, 5 * 24)[0] > p.background[0] + 15);
    }
}

#[test]
fn walls_and_doors_stop_light_while_windows_form_a_cone_through_the_opening() {
    let mut p = fixture();
    let source = point(4., 5.);
    assert!(visible(&p, &Library::new(), source, point(5.5, 5.)));
    assert!(!visible(&p, &Library::new(), source, point(8., 5.)));
    // Both wall endpoints lie far outside the light's bounds.
    assert!(!visible(&p, &Library::new(), source, point(8., 11.)));
    p.levels[0].layers[2].items.push(portal(PortalType::Door));
    assert!(!visible(&p, &Library::new(), source, point(8., 5.)));
    p.levels[0].layers[2].items[0].portal_type = Some(PortalType::Window);
    assert!(visible(&p, &Library::new(), source, point(8., 5.)));
    assert!(!visible(&p, &Library::new(), source, point(8., 1.)));
    assert!(!visible(&p, &Library::new(), source, point(8., 9.)));
    // Hiding a window restores the uncut wall; hiding the wall removes it.
    p.levels[0].layers[2].visible = false;
    assert!(!visible(&p, &Library::new(), source, point(8., 5.)));
    p.levels[0].layers[1].visible = false;
    assert!(visible(&p, &Library::new(), source, point(8., 5.)));
    p.levels[0].layers[1].visible = true;
    if let ItemKind::Wall { blocks_sight, .. } = &mut p.levels[0].layers[1].items[0].kind {
        *blocks_sight = false;
    }
    assert!(visible(&p, &Library::new(), source, point(8., 5.)));
}

#[test]
fn curved_window_removes_all_arc_blockers_while_a_straight_door_blocks_the_chord() {
    let mut p = Project {
        background_asset: None,
        use_default_assets: false,
        ..Project::default()
    };
    let ring = (0..512)
        .map(|n| {
            let a = n as f32 * std::f32::consts::TAU / 512.;
            point(10. + 5. * a.cos(), 10. + 5. * a.sin())
        })
        .collect();
    p.levels[0].layers[1].items.push(Item::new(
        "Round room",
        ItemKind::Room {
            rings: vec![ring],
            floor: None,
            wall: Some("wall".into()),
            width: 0.3,
        },
    ));
    let (attachment, bounds) =
        maploom::editing::attach_portal(&p, 0, point(15., 10.), 1.5).unwrap();
    let mut opening = Item::new(
        "Opening",
        ItemKind::Portal {
            bounds,
            closed: false,
            color: [255; 4],
        },
    );
    opening.wall_attachment = Some(attachment);
    opening.portal_type = Some(PortalType::Window);
    p.levels[0].layers[2].items.push(opening);
    assert!(visible(
        &p,
        &Library::new(),
        point(10., 10.),
        point(17., 10.)
    ));
    assert!(!visible(
        &p,
        &Library::new(),
        point(10., 10.),
        point(17., 13.)
    ));
    p.levels[0].layers[2].items[0].portal_type = Some(PortalType::Door);
    assert!(!visible(
        &p,
        &Library::new(),
        point(10., 10.),
        point(17., 10.)
    ));
    p.levels[0].layers[2].items.clear();
    assert!(!visible(
        &p,
        &Library::new(),
        point(10., 10.),
        point(17., 10.)
    ));
}

#[test]
fn room_perimeters_holes_diagonal_walls_and_other_levels_follow_the_same_rules() {
    let mut p = fixture();
    p.levels[0].layers[1].items = vec![Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![
                vec![
                    point(1., 1.),
                    point(10., 1.),
                    point(10., 10.),
                    point(1., 10.),
                ],
                vec![point(6., 4.), point(7., 4.), point(7., 6.), point(6., 6.)],
            ],
            floor: None,
            wall: Some("wall".into()),
            width: 0.2,
        },
    )];
    assert!(
        !visible(&p, &Library::new(), point(4., 5.), point(8., 5.)),
        "Hole walls block light"
    );
    assert!(visible(&p, &Library::new(), point(4., 5.), point(9., 2.)));
    assert!(
        !visible(&p, &Library::new(), point(4., 5.), point(11., 2.)),
        "Room perimeter blocks light"
    );
    p.levels[0].layers[1].items = vec![wall(point(4., 0.), point(10., 6.))];
    assert!(!visible(&p, &Library::new(), point(3., 5.), point(9., 3.)));
    assert!(visible(&p, &Library::new(), point(3., 5.), point(5., 6.)));
    let other = p.levels[0].clone();
    p.levels.push(other);
    p.levels[0].layers[1].items.clear();
    assert!(
        visible(&p, &Library::new(), point(3., 5.), point(9., 3.)),
        "Other floors cannot cast shadows"
    );
}

#[test]
fn legacy_window_asset_tags_and_explicit_overrides_match_vtt_semantics() {
    let mut p = fixture();
    let mut opening = portal(PortalType::Door);
    opening.portal_type = None;
    opening.texture = Some("opaque".into());
    let meta = AssetMeta {
        id: "opaque".into(),
        name: "Renamed".into(),
        path: "opaque.png".into(),
        tags: vec!["Windows".into()],
        size: point(1., 1.),
        pivot: point(0.5, 0.5),
        sha256: String::new(),
        color_mode: ObjectColorMode::None,
    };
    let library = Library::from([(
        "opaque".into(),
        Asset {
            meta: meta.clone(),
            source: Source::Bytes(Arc::new(vec![])),
        },
    )]);
    p.levels[0].layers[2].items.push(opening);
    assert!(visible(&p, &library, point(4., 5.), point(8., 5.)));
    p.assets.insert("opaque".into(), meta);
    assert!(visible(&p, &Library::new(), point(4., 5.), point(8., 5.)));
    p.levels[0].layers[2].items[0].portal_type = Some(PortalType::Door);
    assert!(!visible(&p, &library, point(4., 5.), point(8., 5.)));
}

#[test]
fn crossing_and_nearby_segments_agree_with_direct_line_of_sight() {
    let mut p = fixture();
    let segments = [
        [point(-20., 2.3), point(20., 2.3)],
        [point(1.4, -4.), point(1.4, 5.)],
        [point(-5., -3.), point(4., 3.)],
        [point(-4., -6.), point(-3., -1.)],
    ];
    p.levels[0].layers[1].items = segments.iter().map(|[a, b]| wall(*a, *b)).collect();
    let polygon = Blockers::new(&p, 0, &Library::new())
        .visibility(point(0., 0.), 8.)
        .unwrap();
    let cross = |a: Point, b: Point| a.x as f64 * b.y as f64 - a.y as f64 * b.x as f64;
    for x in -30..30 {
        for y in -30..30 {
            let target = point(x as f32 * 0.21 + 0.013, y as f32 * 0.21 + 0.027);
            let blocked = segments.iter().any(|[a, b]| {
                let edge = point(b.x - a.x, b.y - a.y);
                let den = cross(target, edge);
                if den.abs() < 1e-10 {
                    return false;
                }
                let t = cross(*a, edge) / den;
                let u = cross(*a, target) / den;
                t > 0. && t < 1. && (0.0..=1.).contains(&u)
            });
            assert_eq!(
                render::contains_polygon(&polygon, target),
                !blocked,
                "Visibility at {target:?}"
            );
        }
    }
    let near = Blockers::new(&fixture(), 0, &Library::new())
        .visibility(point(5.999, 5.), 8.)
        .unwrap();
    assert!(!render::contains_polygon(&near, point(6.1, 5.)));
}

fn library() -> Library {
    ["wall", "portal"]
        .into_iter()
        .map(|id| {
            let mut png = std::io::Cursor::new(vec![]);
            image::RgbaImage::from_pixel(16, 8, image::Rgba([60, 60, 60, 255]))
                .write_to(&mut png, image::ImageFormat::Png)
                .unwrap();
            let bytes = png.into_inner();
            (
                id.into(),
                Asset {
                    meta: AssetMeta {
                        id: id.into(),
                        name: id.into(),
                        path: format!("{id}.png"),
                        tags: vec![],
                        size: point(1., 0.5),
                        pivot: point(0.5, 0.5),
                        sha256: maploom::assets::hash(&bytes),
                        color_mode: ObjectColorMode::None,
                    },
                    source: Source::Bytes(Arc::new(bytes)),
                },
            )
        })
        .collect()
}

fn scene(p: &Project, renderer: &mut SceneRenderer, revision: u64) -> GpuScene {
    renderer
        .scene(
            p,
            &library(),
            SceneOptions {
                level: 0,
                grid: false,
                lights: true,
                revision,
                known_meshes: HashSet::new(),
            },
            &HashSet::new(),
        )
        .unwrap()
}
fn light_covers(scene: &GpuScene, target: Point) -> bool {
    let stamp = &scene
        .textures
        .iter()
        .find(|t| t.width == 512 && t.height == 512)
        .unwrap()
        .key;
    scene
        .meshes
        .iter()
        .filter(|m| &m.texture == stamp)
        .any(|m| {
            let bytes = STANDARD.decode(&m.vertices).unwrap();
            let vertices: Vec<_> = bytes
                .chunks_exact(48)
                .map(|v| {
                    point(
                        f32::from_le_bytes(v[0..4].try_into().unwrap()),
                        f32::from_le_bytes(v[4..8].try_into().unwrap()),
                    )
                })
                .collect();
            STANDARD
                .decode(&m.indices)
                .unwrap()
                .chunks_exact(12)
                .any(|t| {
                    let tri: Vec<_> = t
                        .chunks_exact(4)
                        .map(|i| vertices[u32::from_le_bytes(i.try_into().unwrap()) as usize])
                        .collect();
                    render::contains_polygon(&tri, target)
                })
        })
}

#[test]
fn gpu_and_svg_update_after_moving_deleting_and_retyping_blockers_and_allow_disabling_shadows() {
    let mut p = fixture();
    let mut renderer = SceneRenderer::default();
    let closed = scene(&p, &mut renderer, 0);
    assert!(light_covers(&closed, point(5., 5.)));
    assert!(!light_covers(&closed, point(8., 5.)));
    p.levels[0].layers[2].items.push(portal(PortalType::Window));
    assert!(light_covers(&scene(&p, &mut renderer, 1), point(8., 5.)));
    p.levels[0].layers[2].items[0].portal_type = Some(PortalType::Door);
    assert!(!light_covers(&scene(&p, &mut renderer, 2), point(8., 5.)));
    p.levels[0].layers[1].items[0].translate(point(6., 0.));
    p.levels[0].layers[2].items.clear();
    assert!(light_covers(&scene(&p, &mut renderer, 3), point(8., 5.)));
    p.levels[0].layers[1].items.clear();
    assert!(light_covers(&scene(&p, &mut renderer, 4), point(8., 5.)));
    p.levels[0].layers[1]
        .items
        .push(wall(point(6., -100.), point(6., 100.)));
    let options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: true,
        jpeg_quality: 90,
    };
    let svg = render::svg(&p, &library(), options).unwrap();
    assert!(svg.contains("clipPathUnits=\"userSpaceOnUse\"><polygon"));
    if let ItemKind::Light { shadows, .. } = &mut p.levels[0].layers[0].items[0].kind {
        *shadows = false;
    }
    assert!(light_covers(&scene(&p, &mut renderer, 5), point(8., 5.)));
    assert!(!render::svg(&p, &library(), options)
        .unwrap()
        .contains("clipPathUnits=\"userSpaceOnUse\"><polygon"));
}

#[test]
fn raster_exports_light_only_through_windows_for_every_smoothing_extreme() {
    for smoothing in [0., 1.] {
        let mut p = fixture();
        p.width = 12;
        p.height = 10;
        if let ItemKind::Light { smoothing: s, .. } = &mut p.levels[0].layers[0].items[0].kind {
            *s = smoothing;
        }
        let options = render::ExportOptions {
            level: 0,
            pixels_per_grid: 24,
            grid: false,
            lights: true,
            jpeg_quality: 90,
        };
        let (closed, _) = render::rasterize(&p, &library(), options).unwrap();
        p.levels[0].layers[2].items.push(portal(PortalType::Window));
        let (window, _) = render::rasterize(&p, &library(), options).unwrap();
        assert_eq!(closed.get_pixel(8 * 24, 5 * 24).0, [20, 20, 20, 255]);
        assert!(window.get_pixel(8 * 24, 5 * 24)[0] > 35);
        assert_eq!(window.get_pixel(8 * 24, 24).0, [20, 20, 20, 255]);
    }
}
