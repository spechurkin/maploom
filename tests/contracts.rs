use maploom::{
    assets::{self, Asset, Library, Source},
    document::*,
    formats,
    localization::{Locale, Locales},
    render,
};
use serde_json::json;
use std::{io::Write, path::Path, sync::Arc};
use zip::{ZipWriter, write::SimpleFileOptions};
fn asset() -> Asset {
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::from_pixel(16, 8, image::Rgba([210, 90, 40, 255]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    let hash = assets::hash(&bytes);
    Asset {
        meta: AssetMeta {
            id: "test:object".into(),
            name: "Test asset".into(),
            path: format!("assets/{hash}.png"),
            tags: vec!["test".into()],
            size: Point::new(1., 0.5),
            pivot: Point::new(0.5, 0.5),
            sha256: hash,
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(bytes)),
    }
}
#[test]
fn floors_exclude_every_terrain_material_and_border_variant() {
    let mut meta = asset().meta;
    for name in [
        "grass_tile_1",
        "grass_tile_2",
        "grass_border_1",
        "lava_tile",
        "alien_infestation_tile",
        "acid_tile",
    ] {
        meta.id = format!("res://textures/materials/{name}.png");
        meta.name = name.into();
        meta.tags = vec!["materials".into()];
        assert_ne!(assets::category(&meta), "tilesets");
    }
    meta.id = "res://textures/tilesets/stone_tile.png".into();
    meta.name = "stone_tile".into();
    meta.tags = vec![];
    assert_eq!(assets::category(&meta), "tilesets");
}
#[test]
fn colorable_tags_resolve_pack_local_paths_for_arbitrary_objects() {
    for pack in ["pack-a", "pack-b", "a completely different pack"] {
        let mut a = asset();
        a.meta.id = format!("res://packs/{pack}/textures/objects/generic_item.png");
        let mut other = a.clone();
        other.meta.id = format!("res://packs/{pack}/textures/objects/regular_item.png");
        let mut assets = vec![a, other];
        assets::apply_dd_tags(&mut assets, &serde_json::to_vec(&json!({"tags":{"Colorable":["textures/objects/generic_item.png"],"Furniture":["res://textures/objects/regular_item.png"]}})).unwrap()).unwrap();
        assert_eq!(assets[0].meta.object_color_mode(), ObjectColorMode::RedMask);
        assert_eq!(assets[1].meta.object_color_mode(), ObjectColorMode::None);
        assert!(assets[1].meta.tags.iter().any(|tag| tag == "Furniture"));
    }
}
#[test]
fn red_mask_changes_only_the_paintable_regions_and_preserves_green_leaves() {
    let mut image = image::RgbaImage::from_raw(
        4,
        1,
        vec![
            220, 20, 20, 255, 40, 140, 60, 255, 150, 150, 150, 255, 140, 100, 60, 255,
        ],
    )
        .unwrap();
    assets::recolor_red_mask(&mut image, [20, 80, 220, 255]);
    assert_eq!(image.get_pixel(0, 0).0, [36, 83, 193, 255]);
    assert_eq!(image.get_pixel(1, 0).0, [40, 140, 60, 255]);
    assert_eq!(image.get_pixel(2, 0).0, [150, 150, 150, 255]);
    assert_eq!(image.get_pixel(3, 0).0, [140, 100, 60, 255]);
    assert_ne!(
        assets::color_texture_key("pack-a:item", [255, 0, 0, 255]),
        assets::color_texture_key("pack-b:item", [255, 0, 0, 255])
    );
}
#[test]
fn regular_objects_keep_original_colors_in_the_renderer_and_svg() {
    let mut a = asset();
    a.meta.color_mode = ObjectColorMode::None;
    let mut project = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut item = object(&a);
    item.tint = [0, 255, 0, 255];
    project.levels[0].layers[0].items.push(item);
    project.assets.insert(a.meta.id.clone(), a.meta.clone());
    let ctx = egui::Context::default();
    let texture = ctx.load_texture(
        "regular",
        egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]),
        Default::default(),
    );
    let textures = std::collections::BTreeMap::from([(a.meta.id.clone(), texture.clone())]);
    let output = ctx.run(Default::default(), |ctx| {
        render::draw_scene(
            &ctx.layer_painter(egui::LayerId::background()),
            &project,
            0,
            &textures,
            &render::View {
                origin: egui::Pos2::ZERO,
                scale: 10.,
                grid: false,
                lights: false,
            },
        )
    });
    assert!(output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Mesh(m) if m.texture_id==texture.id() && m.vertices.iter().all(|v|v.color==egui::Color32::WHITE))));
    let library = Library::from([(a.meta.id.clone(), a)]);
    let svg = render::svg(
        &project,
        &library,
        render::ExportOptions {
            level: 0,
            pixels_per_grid: 32,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
        .unwrap();
    assert!(!svg.contains("feColorMatrix"));
}
#[test]
fn independent_room_colors_survive_save_and_render_in_both_gpu_meshes_and_svg() {
    let a = asset();
    let mut wall = a.clone();
    wall.meta.id = "test:wall".into();
    let mut project = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut room = Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![vec![
                Point::new(1., 1.),
                Point::new(5., 1.),
                Point::new(5., 5.),
                Point::new(1., 5.),
            ]],
            floor: Some(a.meta.id.clone()),
            wall: Some(wall.meta.id.clone()),
            width: 0.2,
        },
    );
    room.floor_tint = [150, 90, 40, 255];
    room.wall_tint = [50, 100, 190, 255];
    project.levels[0].layers[0].items.push(room);
    let library = Library::from([
        (a.meta.id.clone(), a.clone()),
        (wall.meta.id.clone(), wall.clone()),
    ]);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("painted.mloom");
    formats::save(&path, &project, &library).unwrap();
    let loaded = formats::load(&path).unwrap();
    let item = &loaded.project.levels[0].layers[0].items[0];
    assert_eq!(item.floor_tint, [150, 90, 40, 255]);
    assert_eq!(item.wall_tint, [50, 100, 190, 255]);
    let ctx = egui::Context::default();
    let image = egui::ColorImage::new([2, 2], vec![egui::Color32::WHITE; 4]);
    let floor_texture = ctx.load_texture("floor", image.clone(), Default::default());
    let wall_texture = ctx.load_texture("wall", image, Default::default());
    let textures = std::collections::BTreeMap::from([
        (a.meta.id.clone(), floor_texture.clone()),
        (wall.meta.id.clone(), wall_texture.clone()),
    ]);
    let output = ctx.run(Default::default(), |ctx| {
        render::draw_scene(
            &ctx.layer_painter(egui::LayerId::background()),
            &loaded.project,
            0,
            &textures,
            &render::View {
                origin: egui::Pos2::ZERO,
                scale: 10.,
                grid: false,
                lights: false,
            },
        )
    });
    for (texture, color) in [
        (floor_texture, [150, 90, 40, 255]),
        (wall_texture, [50, 100, 190, 255]),
    ] {
        assert!(output.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Mesh(m) if m.texture_id==texture.id() && m.vertices.iter().all(|v|v.color.to_srgba_unmultiplied()==color))));
    }
    let svg = render::svg(
        &loaded.project,
        &loaded.library,
        render::ExportOptions {
            level: 0,
            pixels_per_grid: 32,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
        .unwrap();
    assert_eq!(svg.matches("<feColorMatrix").count(), 2);
    let mut old: serde_json::Value = serde_json::to_value(item).unwrap();
    old.as_object_mut().unwrap().remove("floor_tint");
    old.as_object_mut().unwrap().remove("wall_tint");
    let mut old: Item = serde_json::from_value(old).unwrap();
    old.tint = [80, 120, 160, 255];
    old.set_room_tint(true, [160, 100, 50, 255]);
    assert_eq!(old.wall_tint, [80, 120, 160, 255]);
    assert_eq!(old.floor_tint, [160, 100, 50, 255]);
}
#[test]
fn background_texture_repeats_at_constant_scale_when_map_grows() {
    let mut a = asset();
    a.meta.size = Point::new(8., 8.);
    let mut project = Project {
        background_asset: Some(a.meta.id.clone()),
        ..Default::default()
    };
    let library = Library::from([(a.meta.id.clone(), a)]);
    let options = render::ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    for width in [24, 48] {
        project.width = width;
        let svg = render::svg(&project, &library, options).unwrap();
        assert!(svg.contains("patternUnits=\"userSpaceOnUse\""));
        assert!(svg.contains("width=\"8\" height=\"8\""));
    }
    let ctx = egui::Context::default();
    let texture = ctx.load_texture(
        "grass",
        egui::ColorImage::new([2048, 2048], vec![egui::Color32::GREEN; 2048 * 2048]),
        egui::TextureOptions::LINEAR_REPEAT,
    );
    let textures = std::collections::BTreeMap::from([(
        project.background_asset.clone().unwrap(),
        texture.clone(),
    )]);
    let output = ctx.run(Default::default(), |ctx| {
        let painter = ctx.layer_painter(egui::LayerId::background());
        render::draw_scene(
            &painter,
            &project,
            0,
            &textures,
            &render::View {
                origin: egui::Pos2::ZERO,
                scale: 2.,
                grid: false,
                lights: false,
            },
        );
    });
    let uv_max = output
        .shapes
        .iter()
        .find_map(|shape| {
            if let egui::Shape::Mesh(mesh) = &shape.shape {
                if mesh.texture_id == texture.id() {
                    return Some(mesh.vertices.iter().map(|v| v.uv.x).fold(0.0_f32, f32::max));
                }
            }
            None
        })
        .unwrap();
    assert_eq!(uv_max, 6.); // 48 cells / 8 cells per texture, never UV 0..1 stretched.
}
fn object(asset: &Asset) -> Item {
    Item::new(
        "Object",
        ItemKind::Object {
            asset: asset.meta.id.clone(),
            position: Point::new(3., 4.),
            size: asset.meta.size,
            pivot: asset.meta.pivot,
            rotation: 0.4,
            flip_x: true,
            flip_y: false,
        },
    )
}
#[test]
fn project_roundtrip_embeds_original_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let a = asset();
    let mut p = Project::demo();
    p.assets.insert(a.meta.id.clone(), a.meta.clone());
    p.levels[0].layers[0].items.push(object(&a));
    let lib = Library::from([(a.meta.id.clone(), a.clone())]);
    let path = dir.path().join("test.mloom");
    formats::save(&path, &p, &lib).unwrap();
    let loaded = formats::load(&path).unwrap();
    assert_eq!(p, loaded.project);
    assert_eq!(
        assets::verified_bytes(&a).unwrap(),
        assets::verified_bytes(&loaded.library[&a.meta.id]).unwrap()
    );
}
#[test]
fn project_can_replace_its_own_archive() {
    let dir = tempfile::tempdir().unwrap();
    let a = asset();
    let mut p = Project::default();
    p.levels[0].layers[0].items.push(object(&a));
    let lib = Library::from([(a.meta.id.clone(), a)]);
    let path = dir.path().join("test.mloom");
    formats::save(&path, &p, &lib).unwrap();
    let mut loaded = formats::load(&path).unwrap();
    loaded.project.name = "Changed".into();
    formats::save(&path, &loaded.project, &loaded.library).unwrap();
    assert_eq!(formats::load(&path).unwrap().project.name, "Changed");
}
#[test]
fn text_project_is_self_contained() {
    let dir = tempfile::tempdir().unwrap();
    let a = asset();
    let mut p = Project::default();
    p.levels[0].layers[0].items.push(object(&a));
    let lib = Library::from([(a.meta.id.clone(), a.clone())]);
    let path = dir.path().join("readable.mloom.json");
    formats::save(&path, &p, &lib).unwrap();
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(raw["embedded"][&a.meta.id].is_string());
    let loaded = formats::load(&path).unwrap();
    assert_eq!(
        assets::verified_bytes(&a).unwrap(),
        assets::verified_bytes(&loaded.library[&a.meta.id]).unwrap()
    );
}
#[test]
fn fabric_preserves_ids_and_checksums() {
    let dir = tempfile::tempdir().unwrap();
    let a = asset();
    let path = dir.path().join("test.mloon_fabric");
    assets::save_fabric(&path, "Test", "Author", std::slice::from_ref(&a)).unwrap();
    let pack = assets::index_pack(&path).unwrap();
    assert_eq!(pack.assets.len(), 1);
    assert_eq!(pack.assets[0].meta.id, a.meta.id);
    assert_eq!(
        assets::verified_bytes(&a).unwrap(),
        assets::verified_bytes(&pack.assets[0]).unwrap()
    );
}
#[test]
fn pack_import_skips_duplicate_asset_ids_and_keeps_following_assets() {
    let dir = tempfile::tempdir().unwrap();
    let first = asset();
    let mut duplicate = first.meta.clone();
    duplicate.name = "Ignored duplicate".into();
    duplicate.path = "missing-duplicate.png".into();
    let mut last = first.meta.clone();
    last.id = "test:last".into();
    let bytes = assets::verified_bytes(&first).unwrap();
    for (extension, manifest_name, format) in [
        ("mloom_fabric", "fabric.json", "maploom/fabric"),
        ("mloom_prefab", "prefab.json", "maploom/prefab"),
    ] {
        let path = dir.path().join(format!("duplicates.{extension}"));
        let manifest = json!({
            "format": format,
            "version": 1,
            "id": "test:pack",
            "name": "Duplicates",
            "author": "Test",
            "assets": [first.meta, duplicate, last],
        });
        let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
        zip.start_file(&first.meta.path, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
        zip.start_file(manifest_name, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        zip.finish().unwrap();

        let pack = assets::index_pack(&path).unwrap();
        assert_eq!(pack.assets.len(), 2);
        assert_eq!(pack.assets[0].meta, first.meta);
        assert_eq!(pack.assets[1].meta, last);
        for asset in &pack.assets {
            assert_eq!(assets::verified_bytes(asset).unwrap(), bytes);
        }
    }
}
#[test]
fn crop_variants_keep_the_source_immutable() {
    let a = asset();
    let bytes = assets::verified_bytes(&a).unwrap();
    let b = assets::crop_asset(&a, [2, 1, 10, 5], true, true, 1).unwrap();
    assert_ne!(a.meta.id, b.meta.id);
    let image = assets::decode(&assets::verified_bytes(&b).unwrap()).unwrap();
    assert_eq!((image.width(), image.height()), (5, 10));
    assert_eq!(bytes, assets::verified_bytes(&a).unwrap());
    assert!(assets::crop_asset(&a, [0, 0, 100, 100], false, false, 0).is_err());
}
#[test]
fn failed_write_keeps_the_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("save.mloom");
    std::fs::write(&path, b"original").unwrap();
    let result = assets::atomic_write(&path, |f| {
        f.write_all(b"partial")?;
        anyhow::bail!("Injected failure")
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn archive_paths_cannot_escape() {
    for path in [
        "../x",
        "/absolute",
        "C:/x",
        "a\\b",
        "a/../b",
        "a//b",
        "a\0b",
    ] {
        assert!(assets::safe_member(path).is_err(), "{path}")
    }
    assert!(assets::safe_member("images/example.png").is_ok());
}
#[test]
fn malformed_archive_is_rejected_before_loading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.mloom");
    let mut zip = ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("../escape", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"bad").unwrap();
    zip.finish().unwrap();
    assert!(formats::load(&path).unwrap_err_text().contains("Unsafe"));
}
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl<T> ErrorText for anyhow::Result<T> {
    fn unwrap_err_text(self) -> String {
        match self {
            Err(e) => e.to_string(),
            Ok(_) => panic!("Expected error"),
        }
    }
}
#[test]
fn asset_checksum_detects_tampering() {
    let mut a = asset();
    a.source = Source::Bytes(Arc::new(vec![1, 2, 3]));
    assert!(assets::verified_bytes(&a).is_err());
}
#[test]
fn invalid_transforms_and_duplicate_ids_are_rejected() {
    let mut p = Project::demo();
    p.width = 0;
    assert!(p.validate().is_err());
    p.width = 32;
    p.levels[0].layers[1].id = p.levels[0].layers[0].id.clone();
    assert!(p.validate().is_err());
    let mut p = Project::default();
    p.levels[0].layers[0].items.push(Item::new(
        "bad",
        ItemKind::Terrain {
            position: Point::new(f32::NAN, 0.),
            radius: 1.,
            color: [255; 4],
        },
    ));
    assert!(p.validate().is_err());
}
#[test]
fn history_restores_structural_edits_and_clears_redo() {
    let mut p = Project::default();
    let original = p.clone();
    let mut h = History::default();
    h.record(p.clone());
    p.levels.push(Level::new("Upper"));
    assert!(h.undo(&mut p));
    assert_eq!(p, original);
    assert!(h.redo(&mut p));
    assert_eq!(p.levels.len(), 2);
    h.record(p.clone());
    p.name = "branch".into();
    assert!(!h.redo(&mut p));
}
fn dd() -> serde_json::Value {
    json!({"header":{"uses_default_assets":false},"world":{"width":10,"height":8,"levels":{"0":{"label":"Floor","walls":[{"points":"PoolVector2Array( 0, 256, 1024, 256, 1024, 512 )","loop":true,"texture":"res://textures/walls/stone.png","portals":[{"position":"Vector2( 512, 256 )","rotation":0,"radius":128,"texture":"res://textures/portals/door.png","closed":true}]}],"objects":[{"texture":"test:object","position":"Vector2( 256, 512 )","scale":"Vector2( -2, 3 )","rotation":0.5,"layer":200}],"lights":[{"position":"Vector2( 256, 512 )","range":4,"intensity":0.8,"color":"80ff8000"}],"unknown_future_data":{"important":42}}}}})
}
#[test]
fn dd_import_preserves_unknown_data_and_nested_portals() {
    let raw = dd();
    let loaded =
        formats::import_dungeondraft(raw.clone(), Path::new("sample.dungeondraft_map")).unwrap();
    assert_eq!(loaded.project.extensions["dungeondraft_source"], raw);
    assert!(!loaded.project.use_default_assets);
    let items: Vec<_> = loaded.project.levels[0]
        .layers
        .iter()
        .flat_map(|l| &l.items)
        .collect();
    assert_eq!(
        items
            .iter()
            .filter(|i| matches!(i.kind, ItemKind::Portal { .. }))
            .count(),
        1
    );
    let wall = items
        .iter()
        .find(|i| matches!(i.kind, ItemKind::Wall { .. }))
        .unwrap();
    if let ItemKind::Wall { points, .. } = &wall.kind {
        assert_eq!(points.len(), 4);
        assert_eq!(points[0], Point::new(0., 1.));
        assert_eq!(points[0], points[3]);
    }
    assert_eq!(
        wall.texture.as_deref(),
        Some("res://textures/walls/stone.png")
    );
}
#[test]
fn dd_scale_resolves_before_headless_export() {
    let mut loaded = formats::import_dungeondraft(dd(), Path::new("x")).unwrap();
    let a = asset();
    loaded.library.insert(a.meta.id.clone(), a);
    formats::resolve_dd_assets(&mut loaded.project, &loaded.library).unwrap();
    let item = loaded.project.levels[0]
        .layers
        .iter()
        .flat_map(|l| &l.items)
        .find(|i| matches!(i.kind, ItemKind::Object { .. }))
        .unwrap();
    if let ItemKind::Object {
        size,
        flip_x,
        position,
        ..
    } = &item.kind
    {
        assert_eq!(*size, Point::new(0.125, 0.09375));
        assert!(*flip_x);
        assert_eq!(*position, Point::new(1., 2.));
    }
    assert!(
        loaded.project.extensions["dungeondraft_object_scales"]
            .as_object()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn doors_remove_overlapping_wall_intervals() {
    let mut level = Level::new("Floor");
    level.layers[0].items.push(Item::new(
        "Wall",
        ItemKind::Wall {
            points: vec![Point::new(0., 0.), Point::new(10., 0.)],
            width: 0.1,
            color: [255; 4],
            blocks_sight: true,
        },
    ));
    for (a, b) in [(3., 5.), (4., 7.)] {
        level.layers[1].items.push(Item::new(
            "Door",
            ItemKind::Portal {
                bounds: [Point::new(a, 0.), Point::new(b, 0.)],
                closed: true,
                color: [255; 4],
            },
        ));
    }
    let los = formats::line_of_sight(&level);
    assert_eq!(
        los,
        vec![
            vec![Point::new(0., 0.), Point::new(3., 0.)],
            vec![Point::new(7., 0.), Point::new(10., 0.)]
        ]
    );
    level.layers[1].visible = false;
    assert_eq!(formats::line_of_sight(&level).len(), 1);
}
#[test]
fn vtt_uses_grid_units_and_argb_colors() {
    let mut p = Project::default();
    p.levels[0].layers[0].items.push(Item::new(
        "Light",
        ItemKind::Light {
            position: Point::new(2., 3.),
            range: 4.,
            intensity: 1.,
            smoothing: 1.,
            color: [255, 128, 0, 128],
            shadows: true,
        },
    ));
    let v = formats::dd2vtt(&p, 0, 150, b"PNG", &Library::new());
    assert_eq!(v["resolution"]["pixels_per_grid"], 150);
    assert_eq!(v["lights"][0]["position"]["x"], 2.);
    assert_eq!(v["lights"][0]["color"], "80ff8000");
    assert_eq!(v["image"], "UE5H");
}
#[test]
fn concave_floor_triangulation_conserves_area() {
    let p = vec![
        Point::new(0., 0.),
        Point::new(4., 0.),
        Point::new(4., 1.),
        Point::new(1., 1.),
        Point::new(1., 4.),
        Point::new(0., 4.),
    ];
    for p in [p.clone(), p.into_iter().rev().collect()] {
        let triangles = render::triangulate(&p);
        assert_eq!(triangles.len(), 4);
        let area: f32 = triangles
            .iter()
            .map(|t| {
                let a = p[t[0]];
                let b = p[t[1]];
                let c = p[t[2]];
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.
            })
            .sum();
        assert!((area - 7.).abs() < 0.001);
    }
}
#[test]
fn selection_respects_rotation_and_pivot() {
    let mut a = object(&asset());
    if let ItemKind::Object {
        position,
        size,
        pivot,
        rotation,
        ..
    } = &mut a.kind
    {
        *position = Point::new(0., 0.);
        *size = Point::new(4., 1.);
        *pivot = Point::new(0., 0.);
        *rotation = std::f32::consts::FRAC_PI_2;
    }
    assert!(render::hit(&a, Point::new(-0.5, 2.), 0.));
    assert!(!render::hit(&a, Point::new(2., 0.5), 0.));
}
#[test]
fn custom_locales_fall_back_to_english() {
    let locale = Locale::parse(br#"{"format":"maploom/locale","version":1,"id":"xx","name":"Custom","strings":{"Open":"Custom Open","Save":""}}"#).unwrap();
    let mut locales = Locales::default();
    locales.catalogs.insert(locale.id.clone(), locale);
    locales.active = "xx".into();
    assert_eq!(locales.tr("ignored", "Open"), "Custom Open");
    assert_eq!(locales.tr("ignored", "Save"), "Save");
    assert_eq!(
        locales.tr("ignored", "Unknown future key"),
        "Unknown future key"
    );
}
#[test]
fn locales_are_versioned_and_paths_cannot_be_ids() {
    assert!(
        Locale::parse(
            br#"{"format":"maploom/locale","version":1,"id":"../bad","name":"Bad","strings":{}}"#
        )
            .is_err()
    );
    assert!(
        Locale::parse(
            br#"{"format":"maploom/locale","version":2,"id":"de","name":"German","strings":{}}"#
        )
            .is_err()
    );
}
#[test]
fn raw_godot_texture_decodes_without_godot() {
    let mut data = b"GDST".to_vec();
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&0u16.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&3u32.to_le_bytes());
    data.extend_from_slice(&[12, 34, 56, 78]);
    let image = assets::decode(&assets::stex_png(&data).unwrap()).unwrap();
    assert_eq!(image.get_pixel(0, 0).0, [12, 34, 56, 78]);
    assert!(assets::stex_png(&data[..21]).is_err());
}
