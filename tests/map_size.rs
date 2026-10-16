use maploom::{document::*, editing};

fn fixture() -> Project {
    let mut p = Project {
        background_asset: None,
        ..Project::default()
    };
    let mut floor = Item::new(
        "Floor",
        ItemKind::Floor {
            points: vec![
                Point::new(1., 1.),
                Point::new(9., 1.),
                Point::new(9., 9.),
                Point::new(1., 9.),
            ],
            color: [255; 4],
        },
    );
    floor.locked = true;
    floor.floor_holes = vec![vec![
        Point::new(2., 2.),
        Point::new(3., 2.),
        Point::new(3., 3.),
    ]];
    p.levels[0].layers[0].locked = true;
    p.levels[0].layers[0].visible = false;
    p.levels[0].layers[0].items.push(floor);
    let rings = vec![vec![
        Point::new(1., 1.),
        Point::new(9., 1.),
        Point::new(9., 9.),
        Point::new(1., 9.),
    ]];
    let mut water = Item::new(
        "Water",
        ItemKind::TerrainRegion {
            rings: rings.clone(),
            color: [255; 4],
        },
    );
    water.water = Some(WaterStyle {
        deep_color: [55, 159, 154, 255],
        shallow_color: [141, 203, 177, 255],
        blend_distance: 1.5,
        disable_border: false,
        smoothing: 0.2,
    });
    water.water_source_rings = Some(rings);
    p.levels[0].layers[0].items.push(water);
    let mut wall = Item::new(
        "Wall",
        ItemKind::Wall {
            points: vec![Point::new(2., 4.), Point::new(8., 4.)],
            width: 0.2,
            color: [255; 4],
            blocks_sight: true,
        },
    );
    wall.locked = true;
    let mut door = Item::new(
        "Door",
        ItemKind::Portal {
            bounds: [Point::new(4.5, 4.), Point::new(5.5, 4.)],
            closed: true,
            color: [255; 4],
        },
    );
    door.wall_attachment = Some(WallAttachment {
        wall: wall.id.clone(),
        segment: 0,
        t: 0.5,
        length: 1.,
    });
    p.levels[0].layers[1].items.extend([wall, door]);
    let mut second = Level::new("Upstairs");
    second.layers[0].items.push(Item::new(
        "Text",
        ItemKind::Text {
            position: Point::new(7., 8.),
            text: "Test".into(),
            size: 1.,
            color: [255; 4],
        },
    ));
    p.levels.push(second);
    p
}

#[test]
fn each_edge_changes_only_its_boundary_and_shifts_all_levels_together() {
    for (edges, size, delta) in [
        ((3, 0, 0, 0), (35, 27), (0., 3.)),
        ((0, 3, 0, 0), (38, 24), (0., 0.)),
        ((0, 0, -3, 0), (35, 21), (0., 0.)),
        ((0, 0, 0, -3), (32, 24), (-3., 0.)),
        ((2, -4, -1, 3), (34, 25), (3., 2.)),
    ] {
        let mut p = fixture();
        // Fix dimensions explicitly so this test is independent of the default size.
        p.width = 35;
        p.height = 24;
        let mut expected = p.clone();
        expected.width = size.0;
        expected.height = size.1;
        for item in expected
            .levels
            .iter_mut()
            .flat_map(|l| &mut l.layers)
            .flat_map(|l| &mut l.items)
        {
            item.translate(Point::new(delta.0, delta.1));
        }
        editing::resize_map(&mut p, edges.0, edges.1, edges.2, edges.3).unwrap();
        assert_eq!(p, expected);
        assert_eq!(
            p.levels[0].layers[1].items[1].points()[0],
            Point::new(4.5 + delta.0, 4. + delta.1)
        );
    }
}

#[test]
fn crop_keeps_off_map_content_and_roundtrips_through_file_and_history() {
    let original = fixture();
    let mut p = original.clone();
    let mut history = History::default();
    history.record(p.clone());
    editing::resize_map(&mut p, -3, -2, 1, -4).unwrap();
    assert_eq!(
        p.levels[0].layers[0].items[0].points()[0],
        Point::new(-3., -2.)
    );
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("resized.mloom");
    maploom::formats::save(&path, &p, &maploom::assets::Library::new()).unwrap();
    assert_eq!(maploom::formats::load(&path).unwrap().project, p);
    let cropped = p.clone();
    assert!(history.undo(&mut p));
    assert_eq!(p, original);
    assert!(history.redo(&mut p));
    assert_eq!(p, cropped);
    editing::resize_map(&mut p, 3, 2, -1, 4).unwrap();
    assert_eq!(p, original);
}

#[test]
fn rejected_resize_is_atomic_including_overflow_and_bounds() {
    let original = fixture();
    for edges in [
        (0, 513, 0, 0),
        (-512, 0, 0, 0),
        (i32::MAX, i32::MAX, i32::MAX, i32::MAX),
    ] {
        let mut p = original.clone();
        assert!(editing::resize_map(&mut p, edges.0, edges.1, edges.2, edges.3).is_err());
        assert_eq!(p, original);
    }
}

#[test]
fn existing_preferences_gain_empty_presets_and_custom_presets_roundtrip() {
    let mut prefs: maploom::preferences::Preferences =
        serde_json::from_str(r#"{"version":1,"dark":false}"#).unwrap();
    assert!(prefs.map_size_presets.is_empty());
    prefs
        .map_size_presets
        .push(maploom::preferences::MapSizePreset {
            id: "test".into(),
            name: "Table".into(),
            width: 48,
            height: 27,
        });
    let copy: maploom::preferences::Preferences =
        serde_json::from_slice(&serde_json::to_vec(&prefs).unwrap()).unwrap();
    assert!(prefs == copy);
}
