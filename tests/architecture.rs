use maploom::{document::*, editing, formats, geometry, render};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ]
}
fn room() -> Item {
    Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![rect(0., 0., 10., 10.)],
            floor: None,
            wall: Some("wall".into()),
            width: 0.2,
        },
    )
}
#[test]
fn floor_cutouts_survive_native_save_load_translation_and_fill_triangulation() {
    let mut p = Project::default();
    let mut floor = Item::new(
        "Floor",
        ItemKind::Floor {
            points: rect(0., 0., 10., 10.),
            color: [255; 4],
        },
    );
    floor.floor_holes = vec![rect(2., 2., 2., 2.), rect(6., 6., 3., 3.)];
    assert!(!render::hit(&floor, Point::new(3., 3.), 0.));
    assert!(render::hit(&floor, Point::new(1., 1.), 0.));
    floor.translate(Point::new(10., 20.));
    assert!(!render::hit(&floor, Point::new(13., 23.), 0.));
    let area: f32 = geometry::fill_polygons(&floor.floor_rings())
        .iter()
        .map(|r| {
            render::triangulate(r)
                .iter()
                .map(|t| geometry::area(&[r[t[0]], r[t[1]], r[t[2]]]).abs())
                .sum::<f32>()
        })
        .sum();
    assert!((area - 87.).abs() < 0.001, "area={area}");
    p.levels[0].layers[1].items.push(floor.clone());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cutouts.mloom");
    formats::save(&path, &p, &Default::default()).unwrap();
    assert_eq!(
        formats::load(&path).unwrap().project.levels[0].layers[1].items[0],
        floor
    );
}
#[test]
fn contour_wall_switches_control_vtt_blockers_and_keep_floor_holes() {
    let mut i = room();
    if let ItemKind::Room { rings, .. } = &mut i.kind {
        let mut hole = rect(2., 2., 2., 2.);
        hole.reverse();
        rings.push(hole);
        let mut hole = rect(6., 6., 2., 2.);
        hole.reverse();
        rings.push(hole);
    }
    let mut p = Project::default();
    i.wall_exclusions = vec![1];
    p.levels[0].layers[1].items.push(i.clone());
    assert_eq!(formats::line_of_sight(&p.levels[0]).len(), 8);
    assert!(!render::hit(&i, Point::new(3., 3.), 0.));
    i.wall_exclusions = vec![0, 1, 2];
    assert!(i.wall_segments().is_empty());
    p.levels[0].layers[1].items[0] = i;
    assert!(formats::line_of_sight(&p.levels[0]).is_empty());
    p.validate().unwrap();
}
#[test]
fn arbitrary_rooms_are_added_without_rectangular_partitioning() {
    for n in [3, 6, 32, 512] {
        let rings = vec![(0..n)
            .map(|i| {
                let angle = i as f32 * std::f32::consts::TAU / n as f32;
                Point::new(5. + 3. * angle.cos(), 5. + 3. * angle.sin())
            })
            .collect()];
        let mut p = Project::default();
        let i = Item::new(
            "Polygon room",
            ItemKind::Room {
                rings: rings.clone(),
                floor: None,
                wall: Some("wall".into()),
                width: 0.2,
            },
        );
        editing::add_room(&mut p, 0, 1, i).unwrap();
        assert!(
            matches!(&p.levels[0].layers[1].items[0].kind,ItemKind::Room {rings:r,..} if *r==rings)
        );
        let ring = &rings[0];
        let filled: f32 = render::triangulate(ring)
            .iter()
            .map(|t| geometry::area(&[ring[t[0]], ring[t[1]], ring[t[2]]]).abs())
            .sum();
        assert!(
            (filled - geometry::area(ring)).abs() < 0.001,
            "segments={n},area={filled}"
        );
    }
    // Smooth small circles far from the origin must not disappear through
    // cancellation in signed area or a fixed ear-clipping tolerance.
    for radius in [0.1, 0.5, 10.] {
        let ring: Vec<_> = (0..512)
            .map(|n| {
                let a = n as f32 * std::f32::consts::TAU / 512.;
                Point::new(400. + radius * a.cos(), 400. + radius * a.sin())
            })
            .collect();
        let area = geometry::area(&ring);
        let filled: f32 = render::triangulate(&ring)
            .iter()
            .map(|t| geometry::area(&[ring[t[0]], ring[t[1]], ring[t[2]]]).abs())
            .sum();
        assert!(
            (filled - area).abs() < area * 0.001,
            "radius={radius},area={area},filled={filled}"
        );
        assert!((area - std::f32::consts::PI * radius * radius).abs() < area * 0.001);
    }
}
#[test]
fn portal_segments_follow_insertions_deletions_resizing_and_wall_switches() {
    let mut p = Project::default();
    let wall = room();
    let id = wall.id.clone();
    p.levels[0].layers[1].items.push(wall);
    let (attachment, bounds) = editing::attach_portal(&p, 0, Point::new(10., 5.), 1.).unwrap();
    let mut door = Item::new(
        "Door",
        ItemKind::Portal {
            bounds,
            closed: true,
            color: [255; 4],
        },
    );
    door.wall_attachment = Some(attachment);
    p.levels[0].layers[2].items.push(door);
    let mut next = p.clone();
    if let ItemKind::Room { rings, .. } = &mut next.levels[0].layers[1].items[0].kind {
        rings[0].insert(1, Point::new(5., 0.));
    }
    editing::reconcile_geometry(&p, &mut next);
    next.sync_portals();
    assert_eq!(
        next.levels[0].layers[2].items[0]
            .wall_attachment
            .as_ref()
            .unwrap()
            .segment,
        2
    );
    assert_eq!(
        next.levels[0].layers[2].items[0].points(),
        p.levels[0].layers[2].items[0].points()
    );
    p = next.clone();
    if let ItemKind::Room { rings, .. } = &mut next.levels[0].layers[1].items[0].kind {
        for q in &mut rings[0] {
            q.x *= 2.;
        }
    }
    editing::reconcile_geometry(&p, &mut next);
    next.sync_portals();
    assert!(next.levels[0].layers[2].items[0]
        .points()
        .iter()
        .all(|p| (p.x - 20.).abs() < 0.001));
    p = next.clone();
    next.levels[0].layers[1].items[0].wall_exclusions = vec![0];
    editing::reconcile_geometry(&p, &mut next);
    next.sync_portals();
    assert!(next.levels[0].layers[2].items[0].wall_attachment.is_none());
    assert_eq!(next.levels[0].layers[1].items[0].id, id);
}
#[test]
fn old_documents_default_to_solid_floors_and_walled_room_contours() {
    let value = serde_json::to_value(room()).unwrap();
    assert!(value.get("floor_holes").is_none());
    assert!(value.get("wall_exclusions").is_none());
    let i: Item = serde_json::from_value(value).unwrap();
    assert_eq!(i.wall_segments().len(), 4);
    let mut p = Project::default();
    let mut i = room();
    i.wall_exclusions = vec![2];
    p.levels[0].layers[1].items.push(i);
    assert!(p.validate().is_err());
    for hole in [
        rect(-1., 2., 2., 2.),
        rect(0., 2., 2., 2.),
        rect(9., 9., 2., 2.),
    ] {
        assert!(!geometry::valid_floor_cutouts(
            &rect(0., 0., 10., 10.),
            &[hole]
        ));
    }
    assert!(!geometry::valid_floor_cutouts(
        &rect(0., 0., 10., 10.),
        &[rect(2., 2., 4., 4.), rect(3., 3., 1., 1.)]
    ));
}

#[test]
fn floor_cutouts_and_room_wall_switches_match_meshes_svg_and_raster_exports() {
    use maploom::assets::{self, Asset, Library, Source};
    use std::{io::Cursor, sync::Arc};
    let mut bytes = Cursor::new(vec![]);
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 40, 30, 255]))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner();
    let asset = Asset {
        meta: AssetMeta {
            id: "floor".into(),
            name: "Floor".into(),
            path: "floor.png".into(),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(bytes)),
    };
    let library = Library::from([("floor".into(), asset)]);
    for as_room in [false, true] {
        let mut p = Project {
            width: 12,
            height: 12,
            background: [20, 70, 130, 255],
            background_asset: None,
            ..Default::default()
        };
        let mut i = if as_room {
            let mut hole = rect(3., 3., 2., 2.);
            hole.reverse();
            Item::new(
                "Room",
                ItemKind::Room {
                    rings: vec![rect(1., 1., 8., 8.), hole],
                    floor: Some("floor".into()),
                    wall: Some("floor".into()),
                    width: 0.2,
                },
            )
        } else {
            let mut i = Item::new(
                "Floor",
                ItemKind::Floor {
                    points: rect(1., 1., 8., 8.),
                    color: [255; 4],
                },
            )
                .with_texture(Some("floor"));
            i.floor_holes = vec![rect(3., 3., 2., 2.)];
            i
        };
        if as_room {
            i.wall_exclusions = vec![0, 1];
        }
        p.levels[0].layers[1].items.push(i);
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "floor",
            egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]),
            Default::default(),
        );
        let textures = std::collections::BTreeMap::from([("floor".into(), texture.clone())]);
        let output = ctx.run(Default::default(), |ctx| {
            render::draw_scene(
                &ctx.layer_painter(egui::LayerId::background()),
                &p,
                0,
                &textures,
                &render::View {
                    origin: egui::Pos2::ZERO,
                    scale: 1.,
                    grid: false,
                    lights: false,
                },
            )
        });
        let mut area = 0.;
        for shape in output.shapes {
            if let egui::Shape::Mesh(mesh) = shape.shape {
                if mesh.texture_id != texture.id() {
                    continue;
                }
                for t in mesh.indices.chunks_exact(3) {
                    let points = t
                        .iter()
                        .map(|n| {
                            let p = mesh.vertices[*n as usize].pos;
                            Point::new(p.x, p.y)
                        })
                        .collect::<Vec<_>>();
                    area += geometry::area(&points).abs();
                    assert!(!render::contains_polygon(&points, Point::new(4., 4.)));
                }
            }
        }
        assert!((area - 60.).abs() < 0.001, "room={as_room},area={area}");
        let options = render::ExportOptions {
            level: 0,
            pixels_per_grid: 16,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        };
        let svg = render::svg(&p, &library, options).unwrap();
        assert!(svg.contains("fill-rule=\"evenodd\""));
        assert_eq!(svg.matches(" Z ").count(), 2);
        let (image, _) = render::rasterize(&p, &library, options).unwrap();
        assert_eq!(image.get_pixel(4 * 16, 4 * 16).0, p.background);
        assert_eq!(image.get_pixel(2 * 16, 2 * 16).0, [200, 40, 30, 255]);
    }
}
