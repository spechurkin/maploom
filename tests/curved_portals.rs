use maploom::{document::*, editing, formats, openings::Openings, portals};
use std::collections::HashSet;

fn circle(radius: f32, sides: usize) -> Vec<Point> {
    (0..sides)
        .map(|n| {
            let a = n as f32 * std::f32::consts::TAU / sides as f32;
            Point::new(10. + radius * a.cos(), 10. + radius * a.sin())
        })
        .collect()
}
fn room(rings: Vec<Vec<Point>>) -> Item {
    Item::new(
        "Round room",
        ItemKind::Room {
            rings,
            floor: None,
            wall: Some("wall".into()),
            width: 0.3,
        },
    )
}
fn place(project: &mut Project, p: Point, width: f32) -> Item {
    let (attachment, bounds) = editing::attach_portal(project, 0, p, width).unwrap();
    let mut item = Item::new(
        "Window",
        ItemKind::Portal {
            bounds,
            closed: false,
            color: [255; 4],
        },
    );
    item.wall_attachment = Some(attachment);
    item.portal_type = Some(PortalType::Window);
    project.levels[0].layers[2].items.push(item.clone());
    item
}
fn bounds(item: &Item) -> [Point; 2] {
    let ItemKind::Portal { bounds, .. } = item.kind else {
        panic!()
    };
    bounds
}

#[test]
fn smooth_circles_polygons_and_ellipses_keep_chord_width_and_cut_every_covered_edge() {
    for sides in [12, 64, 512] {
        for ellipse in [false, true] {
            for angle in [
                0_f32,
                0.317,
                std::f32::consts::FRAC_PI_2,
                std::f32::consts::PI,
                6.27,
            ] {
                let mut ring = circle(5., sides);
                if ellipse {
                    for p in &mut ring {
                        p.y = 10. + (p.y - 10.) * 0.6;
                    }
                }
                let wall = room(vec![ring]);
                let query = Point::new(
                    10. + 5. * angle.cos(),
                    10. + (if ellipse { 3. } else { 5. }) * angle.sin(),
                );
                let (a, geometry) = portals::attach(&wall, query, 1.5).unwrap();
                assert!((geometry.bounds[0].distance(geometry.bounds[1]) - 1.5).abs() < 1e-4);
                assert!(!geometry.cuts.is_empty());
                for endpoint in geometry.bounds {
                    assert!(wall.wall_segments().iter().any(|l| maploom::render::segment_distance(endpoint, l[0], l[1]) < 1e-5));
                }
                let mut p = Project::default();
                p.levels[0].layers[1].items.push(wall.clone());
                let mut portal = Item::new(
                    "Door",
                    ItemKind::Portal {
                        bounds: geometry.bounds,
                        closed: true,
                        color: [255; 4],
                    },
                );
                portal.wall_attachment = Some(a);
                p.levels[0].layers[2].items.push(portal);
                let openings = Openings::new(&p.levels[0]).for_wall(&wall.id);
                for (line, lo, hi) in geometry.cuts {
                    let removed = 1. - openings.spans(line).iter().map(|(a, b)| b - a).sum::<f32>();
                    assert!((removed - (hi - lo)).abs() < 1e-4);
                }
                // Another wall with identical coordinates must stay solid.
                let other = Openings::new(&p.levels[0]).for_wall("unrelated-wall");
                assert!(
                    wall.wall_segments()
                        .iter()
                        .all(|line| other.spans(*line) == vec![(0., 1.)])
                );
            }
        }
    }
}

#[test]
fn openings_wrap_the_circle_seam_and_stay_on_their_own_room_contour() {
    let mut p = Project::default();
    let wall = room(vec![
        circle(5., 512),
        circle(3., 512).into_iter().rev().collect(),
    ]);
    p.levels[0].layers[1].items.push(wall.clone());
    let door = place(&mut p, Point::new(15., 10.), 2.);
    let geometry = portals::resolve(&wall, door.wall_attachment.as_ref().unwrap()).unwrap();
    assert!(
        geometry
            .cuts
            .iter()
            .any(|(l, ..)| l == &wall.wall_segments()[0])
    );
    assert!(
        geometry
            .cuts
            .iter()
            .any(|(l, ..)| l == &wall.wall_segments()[511])
    );
    let openings = Openings::new(&p.levels[0]).for_wall(&wall.id);
    assert!(
        wall.wall_segments()[512..]
            .iter()
            .all(|l| openings.spans(*l) == vec![(0., 1.)])
    );
    p.levels[0].layers[2].visible = false;
    assert_eq!(
        Openings::new(&p.levels[0])
            .for_wall(&wall.id)
            .spans(wall.wall_segments()[0]),
        vec![(0., 1.)]
    );
}

#[test]
fn short_collinear_edges_and_open_contour_ends_fit_without_changing_asset_width() {
    let wall = Item::new(
        "Subdivided wall",
        ItemKind::Wall {
            points: (0..=40).map(|n| Point::new(n as f32 / 10., 2.)).collect(),
            width: 0.2,
            color: [255; 4],
            blocks_sight: true,
        },
    );
    for x in [0., 0.1, 1.72, 3.99, 4.] {
        let (_, g) = portals::attach(&wall, Point::new(x, 2.), 1.5).unwrap();
        assert!((g.bounds[0].distance(g.bounds[1]) - 1.5).abs() < 1e-5);
        assert!(g.bounds.iter().all(|p| p.x >= 0. && p.x <= 4. && p.y == 2.));
    }
    assert!(portals::attach(&wall, Point::new(2., 2.), 5.).is_none());
    let tiny = room(vec![circle(0.2, 512)]);
    assert!(portals::attach(&tiny, Point::new(10.2, 10.), 1.).is_none());
}

#[test]
fn portals_near_sharp_corners_remain_on_the_selected_side() {
    let wall = room(vec![vec![
        Point::new(0., 0.),
        Point::new(5., 0.),
        Point::new(5., 5.),
        Point::new(0., 5.),
    ]]);
    for p in [Point::new(0.01, 0.), Point::new(4.99, 0.)] {
        let (_, geometry) = portals::attach(&wall, p, 1.5).unwrap();
        assert!(geometry.bounds.iter().all(|p| p.y == 0.));
        assert!((geometry.bounds[0].distance(geometry.bounds[1]) - 1.5).abs() < 1e-5);
        assert_eq!(geometry.cuts.len(), 1);
    }
}

#[test]
fn curved_attachments_survive_edits_moves_duplication_save_and_vtt_export() {
    let mut p = Project {
        background_asset: None,
        use_default_assets: false,
        ..Project::default()
    };
    let wall = room(vec![circle(5., 512)]);
    let wall_id = wall.id.clone();
    p.levels[0].layers[1].items.push(wall);
    let door = place(&mut p, Point::new(15., 10.), 1.5);
    let original = bounds(&door);
    let mut next = p.clone();
    if let ItemKind::Room { rings, .. } = &mut next.levels[0].layers[1].items[0].kind {
        let mid = Point::new(
            (rings[0][100].x + rings[0][101].x) / 2.,
            (rings[0][100].y + rings[0][101].y) / 2.,
        );
        rings[0].insert(101, mid);
    }
    editing::reconcile_geometry(&p, &mut next);
    next.sync_portals();
    assert_eq!(bounds(&next.levels[0].layers[2].items[0]), original);
    p = next;
    editing::move_items(
        &mut p,
        0,
        &HashSet::from([door.id.clone()]),
        Point::new(-0.4, 1.5),
    );
    let moved = &p.levels[0].layers[2].items[0];
    assert!(moved.wall_attachment.is_some());
    assert!(bounds(moved)[0].y > original[0].y + 0.5);
    let ids = HashSet::from([wall_id, door.id]);
    let copies = editing::duplicate_items(&mut p, 0, &ids);
    let copy = p.levels[0].layers[2]
        .items
        .iter()
        .find(|i| copies.contains(&i.id))
        .unwrap();
    assert!(copies.contains(&copy.wall_attachment.as_ref().unwrap().wall));
    let vtt = formats::dd2vtt(&p, 0, 100, b"PNG", &maploom::assets::Library::new());
    assert_eq!(vtt["portals"].as_array().unwrap().len(), 2);
    assert_eq!(vtt["portals"][0]["closed"], false);
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("curved.mloom");
    formats::save(&path, &p, &maploom::assets::Library::new()).unwrap();
    let loaded = formats::load(&path).unwrap();
    assert_eq!(loaded.project, p);
}

#[test]
fn merging_a_curved_room_keeps_a_wide_exterior_door_by_its_contour_anchor() {
    let mut p = Project::default();
    p.levels[0].layers[1]
        .items
        .push(room(vec![circle(2., 512)]));
    let door = place(&mut p, Point::new(12., 10.), 3.5);
    let rectangle = room(vec![vec![
        Point::new(7., 9.),
        Point::new(9., 9.),
        Point::new(9., 11.),
        Point::new(7., 11.),
    ]]);
    editing::add_room(&mut p, 0, 1, rectangle).unwrap();
    let retained = &p.levels[0].layers[2].items[0];
    assert_eq!(retained.id, door.id);
    assert!(retained.wall_attachment.is_some());
    assert!((bounds(retained)[0].distance(bounds(retained)[1]) - 3.5).abs() < 1e-4);
}
