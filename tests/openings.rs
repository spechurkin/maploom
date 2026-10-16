use base64::{engine::general_purpose::STANDARD, Engine};
use maploom::{
    assets::{self, Asset, Library, Source},
    document::*,
    gpu_scene::{SceneOptions, SceneRenderer},
    openings::Openings,
    render::{self, ExportOptions},
};
use std::{collections::HashSet, io::Cursor, sync::Arc};

fn portal(a: Point, b: Point) -> Item {
    Item::new(
        "Door / window",
        ItemKind::Portal {
            bounds: [a, b],
            closed: true,
            color: [255; 4],
        },
    )
        .with_texture(Some("portal"))
}

#[test]
fn cuts_merge_clip_and_follow_visibility_and_collinearity() {
    let mut level = Level::new("Floor");
    for (a, b) in [(-2., 1.), (3., 5.), (7., 4.), (9., 12.)] {
        level.layers[1]
            .items
            .push(portal(Point::new(a, 0.), Point::new(b, 0.)));
    }
    // Nearby parallel and crossing portals must not cut this wall.
    level.layers[1]
        .items
        .push(portal(Point::new(0., 0.1), Point::new(10., 0.1)));
    level.layers[1]
        .items
        .push(portal(Point::new(2., -1.), Point::new(2., 1.)));
    let line = [Point::new(0., 0.), Point::new(10., 0.)];
    assert_eq!(
        Openings::new(&level).spans(line),
        vec![(0.1, 0.3), (0.7, 0.9)]
    );
    assert!(Openings::new(&level).spans([line[0]; 2]).is_empty());
    level.layers[1].visible = false;
    assert_eq!(Openings::new(&level).spans(line), vec![(0., 1.)]);
}

fn asset(id: &str, color: [u8; 4]) -> Asset {
    let mut png = Cursor::new(vec![]);
    image::RgbaImage::from_pixel(16, 8, image::Rgba(color))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    Asset {
        meta: AssetMeta {
            id: id.into(),
            name: id.into(),
            path: format!("{id}.png"),
            tags: vec![],
            size: Point::new(1., 0.5),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(bytes)),
    }
}

fn fixture() -> (Project, Library) {
    let mut p = Project {
        width: 32,
        height: 32,
        background_asset: None,
        background: [210, 180, 140, 255],
        ..Default::default()
    };
    p.levels[0].layers[1].items.push(Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![vec![
                Point::new(2., 2.),
                Point::new(10., 2.),
                Point::new(10., 8.),
                Point::new(2., 8.),
            ]],
            floor: None,
            wall: Some("wall".into()),
            width: 0.5,
        },
    ));
    for points in [
        vec![Point::new(2., 10.), Point::new(10., 10.)],
        vec![Point::new(2., 12.), Point::new(10., 20.)],
        vec![Point::new(10., 22.), Point::new(2., 22.)],
    ] {
        p.levels[0].layers[1].items.push(
            Item::new(
                "Wall",
                ItemKind::Wall {
                    points,
                    width: 0.5,
                    color: [255; 4],
                    blocks_sight: true,
                },
            )
                .with_texture(Some("wall")),
        );
    }
    p.levels[0].layers[1].items.push(
        Item::new(
            "Path",
            ItemKind::Path {
                points: vec![Point::new(2., 24.), Point::new(10., 24.)],
                width: 0.5,
                color: [255; 4],
            },
        )
            .with_texture(Some("wall")),
    );
    for (a, b) in [
        (Point::new(4., 2.), Point::new(6., 2.)),
        (Point::new(10., 4.), Point::new(10., 6.)),
        (Point::new(4., 10.), Point::new(6., 10.)),
        (Point::new(4., 14.), Point::new(6., 16.)),
        (Point::new(6., 22.), Point::new(4., 22.)),
        (Point::new(4., 24.), Point::new(6., 24.)),
    ] {
        p.levels[0].layers[2].items.push(portal(a, b));
    }
    (
        p,
        Library::from([
            ("wall".into(), asset("wall", [50, 60, 70, 255])),
            ("portal".into(), asset("portal", [0; 4])),
        ]),
    )
}

fn wall_covers(p: &Project, library: &Library, point: Point) -> bool {
    let scene = SceneRenderer::default()
        .scene(
            p,
            library,
            SceneOptions {
                level: 0,
                grid: false,
                lights: false,
                revision: 0,
                known_meshes: HashSet::new(),
            },
            &HashSet::new(),
        )
        .unwrap();
    let wall = &scene
        .textures
        .iter()
        .find(|t| {
            t.width == 16
                && image::load_from_memory(&STANDARD.decode(&t.data).unwrap())
                .unwrap()
                .to_rgba8()
                .get_pixel(0, 0)[0]
                == 50
        })
        .unwrap()
        .key;
    scene.meshes.iter().filter(|m| &m.texture == wall).any(|m| {
        let bytes = STANDARD.decode(&m.vertices).unwrap();
        let vertices: Vec<_> = bytes
            .chunks_exact(48)
            .map(|v| {
                Point::new(
                    f32::from_le_bytes(v[0..4].try_into().unwrap()),
                    f32::from_le_bytes(v[4..8].try_into().unwrap()),
                )
            })
            .collect();
        let bytes = STANDARD.decode(&m.indices).unwrap();
        bytes.chunks_exact(12).any(|tri| {
            let v: Vec<_> = tri
                .chunks_exact(4)
                .map(|i| vertices[u32::from_le_bytes(i.try_into().unwrap()) as usize])
                .collect();
            let cross =
                |a: Point, b: Point| (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
            let c = [cross(v[0], v[1]), cross(v[1], v[2]), cross(v[2], v[0])];
            c.iter().all(|v| *v >= -0.00001) || c.iter().all(|v| *v <= 0.00001)
        })
    })
}

#[test]
fn gpu_cuts_rooms_and_walls_in_every_direction_and_restores_removed_portals() {
    let (mut p, library) = fixture();
    for point in [
        Point::new(5., 2.),
        Point::new(10., 5.),
        Point::new(5., 10.),
        Point::new(5., 15.),
        Point::new(5., 22.),
    ] {
        assert!(
            !wall_covers(&p, &library, point),
            "Wall remains at {point:?}"
        );
    }
    for point in [
        Point::new(3., 2.),
        Point::new(10., 3.),
        Point::new(3., 10.),
        Point::new(3., 13.),
        Point::new(3., 22.),
        Point::new(5., 24.),
    ] {
        assert!(
            wall_covers(&p, &library, point),
            "Missing wall/path at {point:?}"
        );
    }
    p.levels[0].layers[2].items.clear();
    assert!(wall_covers(&p, &library, Point::new(5., 2.)));
}

#[test]
fn svg_and_raster_exports_leave_transparent_portals_clear() {
    let (p, library) = fixture();
    let options = ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    let svg = render::svg(&p, &library, options).unwrap();
    assert!(svg.contains("translate(2 2) rotate(0) translate(0 -0.25)\"><rect x=\"0\" width=\"2\""));
    assert!(svg.contains("translate(2 2) rotate(0) translate(0 -0.25)\"><rect x=\"4\" width=\"4\""));
    let (image, _) = render::rasterize(&p, &library, options).unwrap();
    for (x, y) in [(5., 2.), (10., 5.), (5., 10.), (5., 15.), (5., 22.)] {
        let pixel = image.get_pixel((x * 32.) as u32, (y * 32.) as u32).0;
        assert_eq!(
            pixel, p.background,
            "Export still paints wall under portal at {x}, {y}"
        );
    }
    assert_eq!(image.get_pixel(3 * 32, 2 * 32).0, [50, 60, 70, 255]);
}

#[test]
fn curved_openings_remove_arc_meshes_and_raster_pixels_and_restore_after_deletion() {
    let (mut p, library) = fixture();
    for layer in &mut p.levels[0].layers {
        layer.items.clear();
    }
    let ring: Vec<_> = (0..512)
        .map(|n| {
            let angle = n as f32 * std::f32::consts::TAU / 512.;
            Point::new(12. + 6. * angle.cos(), 12. + 6. * angle.sin())
        })
        .collect();
    let wall = Item::new(
        "Round room",
        ItemKind::Room {
            rings: vec![ring],
            floor: None,
            wall: Some("wall".into()),
            width: 0.5,
        },
    );
    p.levels[0].layers[1].items.push(wall);
    let mut points = vec![];
    for angle in [0_f32, 0.9, 2.4, 4.7] {
        let position = Point::new(12. + 6. * angle.cos(), 12. + 6. * angle.sin());
        let mut meta = library["portal"].meta.clone();
        meta.size.x = 1.5;
        let mut window = maploom::editing::portal_item(&p, 0, position, &meta, [255; 4]).unwrap();
        window.portal_type = Some(PortalType::Window);
        p.levels[0].layers[2].items.push(window);
        points.push(position);
    }
    for &point in &points {
        assert!(
            !wall_covers(&p, &library, point),
            "Arc wall remains at {point:?}"
        );
    }
    assert!(wall_covers(&p, &library, Point::new(12., 18.)));
    let options = ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    let (image, _) = render::rasterize(&p, &library, options).unwrap();
    for point in &points {
        assert_eq!(
            image
                .get_pixel((point.x * 32.) as u32, (point.y * 32.) as u32)
                .0,
            p.background
        );
    }
    let svg = render::svg(&p, &library, options).unwrap();
    assert!(svg.contains("<rect x="));
    let vtt = maploom::formats::dd2vtt(&p, 0, 32, b"PNG", &library);
    assert_eq!(vtt["portals"].as_array().unwrap().len(), 4);
    let los = maploom::formats::line_of_sight(&p.levels[0]);
    for point in &points {
        assert!(!los
            .iter()
            .any(|line| render::segment_distance(*point, line[0], line[1]) < 0.02));
    }
    p.levels[0].layers[2].items.clear();
    for point in points {
        assert!(wall_covers(&p, &library, point));
    }
}
