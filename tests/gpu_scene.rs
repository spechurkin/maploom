use base64::{Engine, engine::general_purpose::STANDARD};
use maploom::{
    assets::{self, Asset, Library, Source},
    document::*,
    gpu_scene::{SceneOptions, SceneRenderer},
};
use std::{collections::HashSet, io::Cursor, sync::Arc};
fn asset(id: &str, color: [u8; 4]) -> Asset {
    let mut bytes = Cursor::new(vec![]);
    image::RgbaImage::from_pixel(16, 8, image::Rgba(color))
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner();
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
    let floor = asset("floor", [200, 150, 100, 255]);
    let wall = asset("wall", [80, 80, 80, 255]);
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    p.levels[0].layers[1].items.push(Item::new(
        "Room",
        ItemKind::Room {
            rings: vec![vec![
                Point::new(2., 2.),
                Point::new(8., 2.),
                Point::new(8., 7.),
                Point::new(2., 7.),
            ]],
            floor: Some("floor".into()),
            wall: Some("wall".into()),
            width: 0.2,
        },
    ));
    (
        p,
        Library::from([("floor".into(), floor), ("wall".into(), wall)]),
    )
}
fn options(revision: u64) -> SceneOptions {
    SceneOptions {
        level: 0,
        grid: true,
        lights: true,
        revision,
        known_meshes: HashSet::new(),
    }
}

#[test]
fn oversized_texture_uploads_preserve_aspect_and_detail_in_both_orientations() {
    for (width, height, expected) in [(8192, 256, (4096, 128)), (256, 8192, (128, 4096))] {
        let mut a = asset("large", [255; 4]);
        let mut bytes = Cursor::new(vec![]);
        image::RgbaImage::from_fn(width, height, |x, y| {
            let along = if width > height { x } else { y };
            image::Rgba(if along / 256 % 2 == 0 {
                [230, 20, 40, 255]
            } else {
                [20, 210, 70, 255]
            })
        })
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
        a.meta.size = Point::new(width as f32 / 256., height as f32 / 256.);
        a.meta.sha256 = assets::hash(bytes.get_ref());
        a.source = Source::Bytes(Arc::new(bytes.into_inner()));
        let library = Library::from([("large".into(), a)]);
        let mut p = Project {
            background_asset: None,
            ..Default::default()
        };
        p.levels[0].layers[1].items.push(
            Item::new(
                "Path",
                ItemKind::Path {
                    points: vec![Point::new(1., 2.), Point::new(17., 2.)],
                    width: 0.5,
                    color: [255; 4],
                },
            )
            .with_texture(Some("large")),
        );
        let mut renderer = SceneRenderer::default();
        let scene = renderer
            .scene(&p, &library, options(0), &HashSet::new())
            .unwrap();
        let uploaded = scene
            .textures
            .iter()
            .find(|t| t.format == "image/png")
            .unwrap();
        assert_eq!(
            (uploaded.width, uploaded.height),
            (expected.0 as usize, expected.1 as usize)
        );
        let image = assets::decode(&STANDARD.decode(&uploaded.data).unwrap()).unwrap();
        assert_eq!(image.dimensions(), expected);
        let sample = |along| {
            if width > height {
                image.get_pixel(along, 64).0
            } else {
                image.get_pixel(64, along).0
            }
        };
        assert_eq!(sample(64), [230, 20, 40, 255]);
        assert_eq!(sample(192), [20, 210, 70, 255]);
        let known = scene.used_textures.into_iter().collect();
        let cached = renderer.scene(&p, &library, options(1), &known).unwrap();
        assert_eq!(cached.stats.decoded_images, 0);
    }
}

#[test]
fn terrain_stays_below_floors_and_rooms_without_water_or_caves_in_gpu_and_svg() {
    let library = Library::from([
        ("land".into(), asset("land", [140, 110, 60, 255])),
        ("floor".into(), asset("floor", [100, 100, 100, 255])),
    ]);
    let points = vec![
        Point::new(1., 1.),
        Point::new(8., 1.),
        Point::new(8., 8.),
        Point::new(1., 8.),
    ];
    let first_channel = |data: &str| {
        image::load_from_memory(&STANDARD.decode(data).unwrap())
            .unwrap()
            .to_rgba8()
            .get_pixel(0, 0)[0]
    };
    let mut renderer = SceneRenderer::default();
    for room in [false, true] {
        for terrain_layer in [1, 2] {
            let mut p = Project {
                background_asset: None,
                ..Default::default()
            };
            let kind = if room {
                ItemKind::Room {
                    rings: vec![points.clone()],
                    floor: Some("floor".into()),
                    wall: None,
                    width: 0.2,
                }
            } else {
                ItemKind::Floor {
                    points: points.clone(),
                    color: [255; 4],
                }
            };
            p.levels[0].layers[1]
                .items
                .push(Item::new("Architecture", kind).with_texture(Some("floor")));
            p.levels[0].layers[terrain_layer].items.push(
                Item::new(
                    "Later terrain",
                    ItemKind::TerrainRegion {
                        rings: vec![points.clone()],
                        color: [255; 4],
                    },
                )
                .with_texture(Some("land")),
            );
            let mut opts = options(0);
            opts.grid = false;
            opts.lights = false;
            let scene = renderer.scene(&p, &library, opts, &HashSet::new()).unwrap();
            let key = |color| {
                &scene
                    .textures
                    .iter()
                    .find(|t| {
                        t.format != "rgba" && !t.data.is_empty() && first_channel(&t.data) == color
                    })
                    .unwrap()
                    .key
            };
            let last_land = scene
                .meshes
                .iter()
                .rposition(|m| &m.texture == key(140))
                .unwrap();
            let first_floor = scene
                .meshes
                .iter()
                .position(|m| &m.texture == key(100))
                .unwrap();
            assert!(
                last_land < first_floor,
                "room={room}, terrain_layer={terrain_layer}"
            );
            let svg = maploom::render::svg(
                &p,
                &library,
                maploom::render::ExportOptions {
                    level: 0,
                    pixels_per_grid: 64,
                    grid: false,
                    lights: false,
                    jpeg_quality: 90,
                },
            )
            .unwrap();
            let colors: Vec<_> = svg
                .split("data:image/png;base64,")
                .skip(1)
                .map(|part| first_channel(part.split('"').next().unwrap()))
                .collect();
            assert_eq!(
                colors,
                vec![140, 100],
                "room={room}, terrain_layer={terrain_layer}"
            );
        }
    }
}

#[test]
fn water_stays_above_all_terrain_layers_and_below_architecture_in_gpu_and_svg() {
    let land = asset("land", [140, 110, 60, 255]);
    let floor = asset("floor", [100, 100, 100, 255]);
    let library = Library::from([("land".into(), land), ("floor".into(), floor)]);
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let rings = vec![vec![
        Point::new(1., 1.),
        Point::new(8., 1.),
        Point::new(8., 8.),
        Point::new(1., 8.),
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
        smoothing: 0.6,
    });
    p.levels[0].layers[0].items.push(water);
    p.levels[0].layers[1].items.push(
        Item::new(
            "Floor",
            ItemKind::Floor {
                points: rings[0].clone(),
                color: [255; 4],
            },
        )
        .with_texture(Some("floor")),
    );
    p.levels[0].layers[2].items.push(
        Item::new(
            "Later terrain",
            ItemKind::TerrainRegion {
                rings,
                color: [255; 4],
            },
        )
        .with_texture(Some("land")),
    );
    let scene = SceneRenderer::default()
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    let water_key = &scene
        .textures
        .iter()
        .find(|t| t.width > 600 && t.height > 600)
        .unwrap()
        .key;
    let land_key = &scene
        .textures
        .iter()
        .find(|t| {
            t.width == 16
                && STANDARD.decode(&t.data).is_ok_and(|bytes| {
                    image::load_from_memory(&bytes)
                        .is_ok_and(|im| im.to_rgba8().get_pixel(0, 0)[0] == 140)
                })
        })
        .unwrap()
        .key;
    let first_land = scene
        .meshes
        .iter()
        .position(|m| &m.texture == land_key)
        .unwrap();
    let first_water = scene
        .meshes
        .iter()
        .position(|m| &m.texture == water_key)
        .unwrap();
    assert!(first_land < first_water);
    assert!(
        scene.meshes[first_water + 1..]
            .iter()
            .any(|m| m.texture != *water_key)
    );
    let svg = maploom::render::svg(
        &p,
        &library,
        maploom::render::ExportOptions {
            level: 0,
            pixels_per_grid: 64,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
    .unwrap();
    let water_position = svg.find("<image x=").unwrap();
    assert!(svg[..water_position].contains("patternUnits=\"userSpaceOnUse\""));
    assert!(svg[water_position..].contains("patternUnits=\"userSpaceOnUse\""));
}
#[test]
fn changing_room_colors_reuses_textures_and_geometry_indices() {
    let (mut p, library) = fixture();
    let mut renderer = SceneRenderer::default();
    let cold = renderer
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    assert_eq!(cold.stats.decoded_images, 2);
    let known = cold.used_textures.iter().cloned().collect();
    p.levels[0].layers[1].items[0].floor_tint = [160, 100, 50, 255];
    let warm = renderer.scene(&p, &library, options(1), &known).unwrap();
    assert_eq!(warm.stats.decoded_images, 0);
    assert_eq!(warm.stats.cache_hits, 2);
    assert!(warm.textures.is_empty());
    assert_eq!(
        warm.used_textures.iter().cloned().collect::<HashSet<_>>(),
        known
    );
    assert_eq!(cold.meshes.len(), warm.meshes.len());
    for (a, b) in cold.meshes.iter().zip(&warm.meshes) {
        assert_eq!(a.indices, b.indices);
        assert_eq!(a.texture, b.texture);
        let bytes = STANDARD.decode(&b.vertices).unwrap();
        assert_eq!(bytes.len(), b.vertex_count * 48);
    }
}
#[test]
fn unchanged_meshes_are_referenced_and_a_reload_recovers_their_data() {
    let (mut project, library) = fixture();
    let mut renderer = SceneRenderer::default();
    let first = renderer
        .scene(&project, &library, options(0), &HashSet::new())
        .unwrap();
    let known = first.used_textures.iter().cloned().collect();
    let mesh_keys: HashSet<_> = first.meshes.iter().map(|m| m.key.clone()).collect();
    let mut next = options(1);
    next.known_meshes = mesh_keys.clone();
    let repeated = renderer.scene(&project, &library, next, &known).unwrap();
    assert!(
        repeated
            .meshes
            .iter()
            .all(|m| m.vertices.is_empty() && m.indices.is_empty())
    );
    project.levels[0].layers[1].items[0].floor_tint = [30, 80, 150, 255];
    let mut next = options(2);
    next.known_meshes = mesh_keys;
    let colored = renderer.scene(&project, &library, next, &known).unwrap();
    assert!(colored.meshes.iter().any(|m| !m.vertices.is_empty()));
    assert!(colored.meshes.iter().any(|m| m.vertices.is_empty()));
    assert!(colored.textures.is_empty());
    let reloaded = renderer
        .scene(&project, &library, options(2), &HashSet::new())
        .unwrap();
    assert!(
        reloaded
            .meshes
            .iter()
            .all(|m| !m.vertices.is_empty() && !m.indices.is_empty())
    );
    assert_eq!(reloaded.stats.decoded_images, 0);
    assert_eq!(
        colored.meshes.iter().map(|m| &m.key).collect::<Vec<_>>(),
        reloaded.meshes.iter().map(|m| &m.key).collect::<Vec<_>>()
    );
}
#[test]
fn a_material_swap_only_decodes_the_new_asset_and_undo_is_warm() {
    let (mut p, mut library) = fixture();
    library.insert("new-floor".into(), asset("new-floor", [40, 60, 80, 255]));
    let mut renderer = SceneRenderer::default();
    let first = renderer
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    let mut known: HashSet<_> = first.used_textures.into_iter().collect();
    if let ItemKind::Room { floor, .. } = &mut p.levels[0].layers[1].items[0].kind {
        *floor = Some("new-floor".into());
    }
    let swap = renderer.scene(&p, &library, options(1), &known).unwrap();
    assert_eq!(swap.stats.decoded_images, 1);
    assert_eq!(swap.textures.len(), 1);
    known.extend(swap.used_textures);
    if let ItemKind::Room { floor, .. } = &mut p.levels[0].layers[1].items[0].kind {
        *floor = Some("floor".into());
    }
    let undo = renderer.scene(&p, &library, options(2), &known).unwrap();
    assert_eq!(undo.stats.decoded_images, 0);
    assert!(undo.textures.is_empty());
}
#[test]
fn a_client_can_recover_textures_after_a_cancelled_response() {
    let (p, library) = fixture();
    let mut renderer = SceneRenderer::default();
    let first = renderer
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    let retry = renderer
        .scene(&p, &library, options(1), &HashSet::new())
        .unwrap();
    assert_eq!(retry.stats.decoded_images, 0);
    assert_eq!(first.textures.len(), retry.textures.len());
    assert!(retry.textures.iter().all(|t| !t.data.is_empty()));
}
#[test]
fn replacing_an_asset_with_the_same_id_invalidates_its_image() {
    let (p, mut library) = fixture();
    let mut renderer = SceneRenderer::default();
    let before = renderer
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    let known = before.used_textures.into_iter().collect();
    library.insert("floor".into(), asset("floor", [10, 80, 120, 255]));
    let after = renderer.scene(&p, &library, options(1), &known).unwrap();
    assert_eq!(after.stats.decoded_images, 1);
    assert_eq!(after.textures.len(), 1);
}
#[test]
fn red_mask_color_changes_are_shader_data_and_do_not_create_new_images() {
    let (mut p, mut library) = fixture();
    let mut a = asset("paintable", [220, 20, 20, 255]);
    a.meta.color_mode = ObjectColorMode::RedMask;
    library.insert("paintable".into(), a);
    let mut object = Item::new(
        "Paintable",
        ItemKind::Object {
            asset: "paintable".into(),
            position: Point::new(4., 4.),
            size: Point::new(2., 1.),
            rotation: 0.,
            pivot: Point::new(0.5, 0.5),
            flip_x: false,
            flip_y: false,
        },
    );
    object.tint = [20, 80, 220, 255];
    p.levels[0].layers[2].items.push(object);
    let mut renderer = SceneRenderer::default();
    let first = renderer
        .scene(&p, &library, options(0), &HashSet::new())
        .unwrap();
    assert!(first.meshes.iter().any(|m| m.mode == 1));
    assert!(first.textures.iter().any(|t| t.alias.is_some()));
    let known = first.used_textures.into_iter().collect();
    p.levels[0].layers[2].items[0].tint = [220, 100, 20, 255];
    let after = renderer.scene(&p, &library, options(1), &known).unwrap();
    assert_eq!(after.stats.decoded_images, 0);
    assert!(after.textures.is_empty());
    assert!(after.meshes.iter().any(|m| m.mode == 1));
}
#[test]
fn terrain_tiles_are_opaque_but_borders_keep_their_coverage() {
    let tile = asset("res://textures/terrain/test.png", [20, 40, 60, 46]);
    let border = asset("res://textures/materials/test_border.png", [80, 60, 40, 70]);
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    let mut i = Item::new(
        "Paint",
        ItemKind::Terrain {
            position: Point::new(4., 4.),
            radius: 2.,
            color: [255; 4],
        },
    );
    i.texture = Some(tile.meta.id.clone());
    i.border = Some(border.meta.id.clone());
    p.levels[0].layers[0].items.push(i);
    p.migrate_terrain();
    let library = Library::from([
        (tile.meta.id.clone(), tile),
        (border.meta.id.clone(), border),
    ]);
    let mut renderer = SceneRenderer::default();
    let scene = renderer
        .scene(
            &p,
            &library,
            SceneOptions {
                grid: false,
                lights: false,
                ..options(0)
            },
            &HashSet::new(),
        )
        .unwrap();
    let images: Vec<_> = scene
        .textures
        .iter()
        .filter(|t| t.width == 16 && t.height == 8)
        .map(|t| {
            let bytes = STANDARD.decode(&t.data).unwrap();
            assets::decode(&bytes).unwrap()
        })
        .collect();
    assert!(
        images
            .iter()
            .any(|i| i.pixels().all(|p| p.0 == [20, 40, 60, 255]))
    );
    assert!(
        images
            .iter()
            .any(|i| i.pixels().all(|p| p.0 == [80, 60, 40, 70]))
    );
    let svg = maploom::render::svg(
        &p,
        &library,
        maploom::render::ExportOptions {
            level: 0,
            pixels_per_grid: 16,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
    .unwrap();
    let mut embedded = svg.split("data:image/png;base64,").skip(1).map(|s| {
        let bytes = STANDARD.decode(s.split('"').next().unwrap()).unwrap();
        assets::decode(&bytes).unwrap()
    });
    assert!(embedded.any(|i| i.pixels().all(|p| p.0 == [20, 40, 60, 255])));
}

#[test]
fn returning_to_a_terrain_material_keeps_paint_order_in_gpu_and_svg() {
    let a = asset("a", [200, 40, 30, 255]);
    let b = asset("b", [20, 70, 200, 255]);
    let library = Library::from([("a".into(), a), ("b".into(), b)]);
    let mut p = Project {
        background_asset: None,
        ..Default::default()
    };
    for (id, x) in [("a", 3.), ("a", 4.), ("b", 4.), ("a", 4.)] {
        let mut i = Item::new(
            id,
            ItemKind::Terrain {
                position: Point::new(x, 4.),
                radius: 2.,
                color: [255; 4],
            },
        );
        i.texture = Some(id.into());
        p.levels[0].layers[0].items.push(i);
    }
    p.migrate_terrain();
    assert_eq!(p.levels[0].layers[0].items.len(), 3);
    let mut renderer = SceneRenderer::default();
    let scene = renderer
        .scene(
            &p,
            &library,
            SceneOptions {
                grid: false,
                lights: false,
                ..options(0)
            },
            &HashSet::new(),
        )
        .unwrap();
    // The adjacent first two stamps merge, the last red stroke remains above blue.
    let materials: Vec<_> = scene
        .meshes
        .iter()
        .filter(|m| {
            scene
                .textures
                .iter()
                .any(|t| t.key == m.texture && t.width == 16)
        })
        .map(|m| &m.texture)
        .collect();
    assert_eq!(materials.len(), 3);
    assert_eq!(materials[0], materials[2]);
    assert_ne!(materials[0], materials[1]);
    let svg = maploom::render::svg(
        &p,
        &library,
        maploom::render::ExportOptions {
            level: 0,
            pixels_per_grid: 16,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
    .unwrap();
    assert_eq!(svg.matches("<pattern ").count(), 3);
    // A floor is painted after all terrain. Distinct owners keep separate meshes
    // even when their textures match, so pixel picking can distinguish them.
    let mut floor = Item::new(
        "Floor",
        ItemKind::Floor {
            points: vec![Point::new(1., 1.), Point::new(7., 1.), Point::new(7., 7.)],
            color: [255; 4],
        },
    );
    floor.texture = Some("b".into());
    p.levels[0].layers[0].items[1] = floor;
    let next = renderer
        .scene(
            &p,
            &library,
            SceneOptions {
                grid: false,
                lights: false,
                ..options(1)
            },
            &HashSet::new(),
        )
        .unwrap();
    let order: Vec<_> = next
        .meshes
        .iter()
        .filter(|m| {
            next.textures
                .iter()
                .any(|t| t.key == m.texture && t.width == 16)
        })
        .map(|m| &m.texture)
        .collect();
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], materials[0]);
    assert_eq!(order[1], materials[0]);
    assert_eq!(order[2], materials[1]);
}
