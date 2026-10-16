use maploom::{document::*, editing, geometry};
use std::collections::HashSet;
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
            wall: Some("wall".into()),
            width: 0.2,
        },
    )
}
#[test]
fn lock_guard_protects_items_even_when_moved_to_other_layers() {
    let mut p = Project::default();
    let mut i = room(0., 0., 4., 4.);
    i.locked = true;
    p.levels[0].layers[1].items.push(i);
    let mut next = p.clone();
    next.levels[0].layers[1].items[0].translate(Point::new(1., 0.));
    assert!(editing::protect_locked(&p, &next).is_err());
    let mut next = p.clone();
    next.levels[0].layers[1].items[0].locked = false;
    assert!(editing::protect_locked(&p, &next).is_ok());
    next.levels[0].layers[1].items[0].name = "changed".into();
    assert!(editing::protect_locked(&p, &next).is_err());
}
#[test]
fn group_operations_skip_locked_layers_and_items() {
    let mut p = Project::default();
    let a = room(0., 0., 4., 4.);
    let mut b = room(6., 0., 4., 4.);
    b.locked = true;
    let ids = HashSet::from([a.id.clone(), b.id.clone()]);
    p.levels[0].layers[1].items = vec![a.clone(), b.clone()];
    editing::move_items(&mut p, 0, &ids, Point::new(1., 2.));
    assert_eq!(
        p.levels[0].layers[1].items[0].points()[0],
        Point::new(1., 2.)
    );
    assert_eq!(p.levels[0].layers[1].items[1], b);
    let copies = editing::duplicate_items(&mut p, 0, &ids);
    assert_eq!(copies.len(), 1);
    assert_eq!(p.levels[0].layers[1].items.len(), 3);
    editing::delete_items(&mut p, 0, &ids);
    assert_eq!(p.levels[0].layers[1].items.len(), 2);
    assert!(p.levels[0].layers[1].items.contains(&b));
    p.levels[0].layers[1].locked = true;
    let before = p.clone();
    let ids = p.levels[0].layers[1]
        .items
        .iter()
        .map(|i| i.id.clone())
        .collect();
    editing::move_items(&mut p, 0, &ids, Point::new(9., 9.));
    editing::delete_items(&mut p, 0, &ids);
    assert_eq!(p, before);
}
#[test]
fn door_attachment_survives_room_move_and_group_duplication() {
    let mut p = Project::default();
    let wall = room(0., 0., 4., 4.);
    let wall_id = wall.id.clone();
    p.levels[0].layers[1].items.push(wall);
    let (attachment, bounds) = editing::attach_portal(&p, 0, Point::new(2., 0.), 1.).unwrap();
    let mut door = Item::new(
        "Door",
        ItemKind::Portal {
            bounds,
            closed: true,
            color: [255; 4],
        },
    );
    door.wall_attachment = Some(attachment);
    let door_id = door.id.clone();
    p.levels[0].layers[1].items.push(door);
    let ids = HashSet::from([wall_id, door_id]);
    editing::move_items(&mut p, 0, &ids, Point::new(3., 2.));
    let ItemKind::Portal { bounds, .. } = p.levels[0].layers[1].items[1].kind else {
        panic!()
    };
    assert_eq!(bounds, [Point::new(4.5, 2.), Point::new(5.5, 2.)]);
    let copies = editing::duplicate_items(&mut p, 0, &ids);
    let door = p.levels[0].layers[1]
        .items
        .iter()
        .find(|i| copies.contains(&i.id) && i.wall_attachment.is_some())
        .unwrap();
    assert!(copies.contains(&door.wall_attachment.as_ref().unwrap().wall));
}
#[test]
fn connected_rooms_merge_transitively_and_preserve_locked_rooms() {
    let mut layer = Layer::new("Architecture");
    layer.items.push(room(0., 0., 4., 4.));
    layer.items.push(room(6., 0., 4., 4.));
    let mut locked = room(2., 0., 4., 4.);
    locked.locked = true;
    layer.items.push(locked.clone());
    let mut p = Project::default();
    p.levels[0].layers[1] = layer;
    editing::add_room(&mut p, 0, 1, room(3., 0., 4., 4.)).unwrap();
    let layer = &p.levels[0].layers[1];
    assert_eq!(layer.items.len(), 2);
    assert!(layer.items.contains(&locked));
    let merged = layer.items.iter().find(|i| !i.locked).unwrap();
    let ItemKind::Room { rings, .. } = &merged.kind else {
        panic!()
    };
    assert!((rings.iter().map(|r| geometry::area(r).abs()).sum::<f32>() - 40.).abs() < 0.1);
}
#[test]
fn a_new_edit_after_undo_discards_redo_branch() {
    let mut history = History::default();
    let mut p = Project::default();
    let original = p.clone();
    history.record(p.clone());
    p.name = "changed".into();
    assert!(history.can_undo());
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(history.can_redo());
    history.record(p.clone());
    p.name = "different".into();
    assert!(!history.can_redo());
}
