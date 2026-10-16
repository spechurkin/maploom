use maploom::{document::*, editing};
fn room(x: f32, y: f32, w: f32, h: f32) -> Item {
    Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![vec![
                Point::new(x, y),
                Point::new(x + w, y),
                Point::new(x + w, y + h),
                Point::new(x, y + h),
            ]],
            floor: None,
            wall: Some("stone".into()),
            width: 0.2,
        },
    )
}
#[test]
fn merge_reattaches_external_doors_across_layers_and_removes_internal_doors() {
    let mut p = Project::default();
    p.levels[0].layers[1].items.push(room(0., 0., 4., 4.));
    let door = |position| {
        let (a, b) = editing::attach_portal(&p, 0, position, 1.).unwrap();
        let mut i = Item::new(
            "Door",
            ItemKind::Portal {
                bounds: b,
                closed: true,
                color: [255; 4],
            },
        );
        i.wall_attachment = Some(a);
        i
    };
    let exterior = door(Point::new(0., 2.));
    let exterior_id = exterior.id.clone();
    let interior = door(Point::new(4., 2.));
    let interior_id = interior.id.clone();
    p.levels[0].layers[2].items.extend([exterior, interior]);
    let next = room(3., 0., 4., 4.);
    let new_id = next.id.clone();
    editing::add_room(&mut p, 0, 1, next).unwrap();
    assert_eq!(p.levels[0].layers[1].items.len(), 1);
    assert_eq!(p.levels[0].layers[2].items.len(), 1);
    let exterior = &p.levels[0].layers[2].items[0];
    assert_eq!(exterior.id, exterior_id);
    assert_eq!(exterior.wall_attachment.as_ref().unwrap().wall, new_id);
    assert_ne!(exterior.id, interior_id);
    let ItemKind::Room { rings, .. } = &p.levels[0].layers[1].items[0].kind else {
        panic!()
    };
    assert_eq!(rings.len(), 1);
    assert!(
        !p.levels[0].layers[1].items[0]
            .wall_segments()
            .iter()
            .any(|s| s[0].x == 4. && s[1].x == 4.)
    );
}

fn circular_room(x: f32, y: f32, radius: f32) -> Item {
    let mut item = room(0., 0., 1., 1.);
    if let ItemKind::Room { rings, .. } = &mut item.kind {
        *rings = vec![
            (0..512)
                .map(|n| {
                    let angle = n as f32 * std::f32::consts::TAU / 512.;
                    Point::new(x + radius * angle.cos(), y + radius * angle.sin())
                })
                .collect(),
        ];
    }
    item
}

#[test]
fn drawing_a_circle_over_a_rectangle_merges_in_either_order() {
    for circle_first in [false, true] {
        let mut p = Project::default();
        let rectangle = room(0., 0., 4., 4.);
        let circle = circular_room(4., 2., 2.);
        let (first, next) = if circle_first {
            (circle, rectangle)
        } else {
            (rectangle, circle)
        };
        p.levels[0].layers[1].items.push(first);
        editing::add_room(&mut p, 0, 1, next).unwrap();
        assert_eq!(p.levels[0].layers[1].items.len(), 1);
        let ItemKind::Room { rings, .. } = &p.levels[0].layers[1].items[0].kind else {
            panic!()
        };
        assert_eq!(rings.len(), 1);
        assert!(
            (maploom::geometry::area(&rings[0]) - (16. + 2. * std::f32::consts::PI)).abs() < 0.001
        );
        p.validate().unwrap();
    }
}

#[test]
fn a_curved_merge_keeps_exterior_doors_and_removes_internal_doors() {
    let mut p = Project::default();
    p.levels[0].layers[1].items.push(room(0., 0., 4., 4.));
    let mut door_ids = vec![];
    let mut exterior_points = vec![];
    for position in [Point::new(0., 2.), Point::new(4., 2.)] {
        let (attachment, bounds) = editing::attach_portal(&p, 0, position, 1.).unwrap();
        let mut door = Item::new(
            "Door",
            ItemKind::Portal {
                bounds,
                closed: true,
                color: [255; 4],
            },
        );
        door.wall_attachment = Some(attachment);
        if door_ids.is_empty() {
            exterior_points = door.points();
        }
        door_ids.push(door.id.clone());
        p.levels[0].layers[2].items.push(door);
    }
    let next = circular_room(4., 2., 2.);
    let id = next.id.clone();
    editing::add_room(&mut p, 0, 1, next).unwrap();
    assert_eq!(p.levels[0].layers[1].items.len(), 1);
    assert_eq!(p.levels[0].layers[2].items.len(), 1);
    let door = &p.levels[0].layers[2].items[0];
    assert_eq!(door.id, door_ids[0]);
    assert_eq!(door.wall_attachment.as_ref().unwrap().wall, id);
    assert_eq!(door.points(), exterior_points);
    p.validate().unwrap();
}

#[test]
fn curved_rooms_merge_transitively_but_skip_locked_and_separate_rooms() {
    let mut p = Project::default();
    // The left room is visited before the bridge that connects it to the circle.
    p.levels[0].layers[1].items.push(room(0., 0., 2., 4.));
    p.levels[0].layers[1].items.push(room(2., 0., 2., 4.));
    let separate = room(20., 20., 2., 2.);
    let mut locked = room(4., 0., 2., 4.);
    locked.locked = true;
    p.levels[0].layers[1]
        .items
        .extend([separate.clone(), locked.clone()]);
    editing::add_room(&mut p, 0, 1, circular_room(5., 2., 2.)).unwrap();
    let items = &p.levels[0].layers[1].items;
    assert_eq!(items.len(), 3);
    assert_eq!(items[0], separate);
    assert_eq!(items[1], locked);
    let ItemKind::Room { rings, .. } = &items[2].kind else {
        panic!()
    };
    assert_eq!(rings.len(), 1);
    assert!(maploom::geometry::inside(rings, Point::new(1., 2.)));
    assert!(maploom::geometry::inside(rings, Point::new(6., 2.)));
}
