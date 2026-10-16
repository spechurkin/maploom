use maploom::{assets::Library, document::*, editing, formats, geometry, render};
use std::collections::HashSet;

fn stamp(x: f32, texture: &str) -> Item {
    Item::new(
        "Terrain",
        ItemKind::Terrain {
            position: Point::new(x, 4.),
            radius: 1.,
            color: [255; 4],
        },
    )
        .with_texture(Some(texture))
}

#[test]
fn migration_groups_legacy_stamps_without_crossing_materials_or_locks() {
    let mut p = Project::default();
    let a = stamp(2., "a");
    let id = a.id.clone();
    let mut locked = stamp(3., "a");
    locked.locked = true;
    p.levels[0].layers[0].items = vec![a, stamp(3., "a"), stamp(3., "b"), stamp(3., "a"), locked];
    p.migrate_terrain();
    let items = &p.levels[0].layers[0].items;
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].id, id);
    assert_eq!(
        items
            .iter()
            .map(|i| i.texture.as_deref().unwrap())
            .collect::<Vec<_>>(),
        vec!["a", "b", "a", "a"]
    );
    assert!(items[3].locked);
    assert!(items
        .iter()
        .all(|i| matches!(i.kind, ItemKind::TerrainRegion { .. })));
    assert!(render::hit(&items[0], Point::new(2., 4.), 0.));
    assert!(render::hit(&items[0], Point::new(3., 4.), 0.));
    let migrated = p.clone();
    p.migrate_terrain();
    assert_eq!(p, migrated);
}

#[test]
fn terrain_moves_duplicates_and_undoes_as_one_object_and_preserves_holes() {
    let mut p = Project::default();
    let stamps = (0..80)
        .map(|i| {
            let angle = i as f32 / 80. * std::f32::consts::TAU;
            (
                Point::new(5. + 3. * angle.cos(), 5. + 3. * angle.sin()),
                0.6,
            )
        })
        .collect::<Vec<_>>();
    let item = Item::new(
        "Ring",
        ItemKind::TerrainRegion {
            rings: geometry::terrain_contours(&stamps),
            color: [255; 4],
        },
    );
    assert!(!render::hit(&item, Point::new(5., 5.), 0.));
    assert!(render::hit(&item, Point::new(8., 5.), 0.));
    let ids = HashSet::from([item.id.clone()]);
    p.levels[0].layers[0].items.push(item);
    let before = p.clone();
    let mut history = History::default();
    history.record(p.clone());
    editing::move_items(&mut p, 0, &ids, Point::new(4., 0.));
    assert!(render::hit(
        &p.levels[0].layers[0].items[0],
        Point::new(12., 5.),
        0.
    ));
    assert!(history.undo(&mut p));
    assert_eq!(p, before);
    assert!(history.redo(&mut p));
    editing::duplicate_items(&mut p, 0, &ids);
    assert_eq!(p.levels[0].layers[0].items.len(), 2);
    editing::delete_items(&mut p, 0, &ids);
    assert_eq!(p.levels[0].layers[0].items.len(), 1);
    p.validate().unwrap();
}

#[test]
fn old_json_and_archives_save_and_reopen_as_contours_without_brush_samples() {
    let temp = tempfile::tempdir().unwrap();
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut a = stamp(2., "a");
    a.texture = None;
    let mut b = a.clone();
    b.id = id();
    b.translate(Point::new(1., 0.));
    p.levels[0].layers[0].items = vec![a, b];
    let legacy = temp.path().join("legacy.json");
    std::fs::write(&legacy, serde_json::to_vec(&p).unwrap()).unwrap();
    let loaded = formats::load(&legacy).unwrap().project;
    assert_eq!(loaded.levels[0].layers[0].items.len(), 1);
    for ext in ["json", "mloom"] {
        let path = temp.path().join(format!("region.{ext}"));
        formats::save(&path, &p, &Library::new()).unwrap();
        let reopened = formats::load(&path).unwrap().project;
        assert_eq!(reopened, loaded);
        let encoded = serde_json::to_string(&reopened).unwrap();
        assert!(encoded.contains("terrain_region"));
        assert!(!encoded.contains("\"radius\""));
    }
}
