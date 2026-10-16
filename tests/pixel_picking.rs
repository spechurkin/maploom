use base64::{engine::general_purpose::STANDARD, Engine};
use maploom::{
    assets::{self, Asset, Library, Source},
    document::*,
    gpu_scene::{SceneOptions, SceneRenderer},
};
use serde_json::json;
use std::{collections::HashSet, io::Cursor, sync::Arc};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Vec<Point> {
    vec![
        Point::new(x, y),
        Point::new(x + w, y),
        Point::new(x + w, y + h),
        Point::new(x, y + h),
    ]
}
fn item(id: &str, kind: serde_json::Value, texture: Option<&str>) -> Item {
    let mut item = Item::new(id, serde_json::from_value(kind).unwrap());
    item.id = id.into();
    item.texture = texture.map(str::to_owned);
    item
}
fn asset(id: &str, hole: bool) -> Asset {
    let image = image::RgbaImage::from_fn(64, 64, |x, y| {
        image::Rgba([
            255,
            0,
            0,
            if hole && (16..48).contains(&x) && (16..48).contains(&y) {
                0
            } else {
                255
            },
        ])
    });
    let mut bytes = Cursor::new(vec![]);
    image.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    let bytes = bytes.into_inner();
    Asset {
        meta: AssetMeta {
            id: id.into(),
            name: id.into(),
            path: format!("{id}.png"),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: if id == "mask" {
                ObjectColorMode::RedMask
            } else {
                ObjectColorMode::Tint
            },
        },
        source: Source::Bytes(Arc::new(bytes)),
    }
}

#[test]
fn every_rendered_type_keeps_its_owner_including_early_return_branches_and_cached_meshes() {
    let mut project = Project {
        width: 64,
        height: 64,
        background_asset: None,
        ..Default::default()
    };
    let library = Library::from([
        ("solid".into(), asset("solid", false)),
        ("cutout".into(), asset("cutout", true)),
        ("mask".into(), asset("mask", true)),
    ]);
    let mut items = vec![
        item(
            "room",
            json!({"type":"room","rings":[rect(2.,2.,4.,4.)],"floor":null,"wall":"solid","width":1}),
            None,
        ),
        item(
            "floor",
            json!({"type":"floor","points":rect(10.,2.,6.,6.),"color":[255,255,255,255]}),
            Some("solid"),
        ),
        item(
            "wall",
            json!({"type":"wall","points":[{"x":2,"y":12},{"x":8,"y":12},{"x":8,"y":18}],"width":1,"color":[255,255,255,255],"blocks_sight":true}),
            Some("solid"),
        ),
        item(
            "path",
            json!({"type":"path","points":[{"x":12,"y":12},{"x":18,"y":12}],"width":2,"color":[255,255,255,255]}),
            Some("cutout"),
        ),
        item(
            "terrain",
            json!({"type":"terrain_region","rings":[rect(22.,2.,6.,6.)],"color":[255,255,255,255]}),
            Some("solid"),
        ),
        item(
            "water",
            json!({"type":"terrain_region","rings":[rect(32.,2.,6.,6.),rect(34.,4.,2.,2.)],"color":[255,255,255,255]}),
            None,
        ),
        item(
            "cave",
            json!({"type":"terrain_region","rings":[rect(42.,2.,6.,6.)],"color":[255,255,255,255]}),
            Some("solid"),
        ),
        item(
            "object",
            json!({"type":"object","asset":"cutout","position":{"x":24,"y":14},"size":{"x":4,"y":4},"rotation":0.6,"pivot":{"x":0.5,"y":0.5},"flip_x":true,"flip_y":true}),
            None,
        ),
        item(
            "portal",
            json!({"type":"portal","bounds":[{"x":32,"y":14},{"x":36,"y":14}],"closed":true,"color":[255,255,255,255]}),
            Some("cutout"),
        ),
        item(
            "light",
            json!({"type":"light","position":{"x":44,"y":14},"range":3,"intensity":1,"color":[255,255,255,255],"shadows":false}),
            None,
        ),
        item(
            "text",
            json!({"type":"text","position":{"x":8,"y":24},"text":"O O","size":2,"color":[255,255,255,255]}),
            None,
        ),
        item(
            "roof",
            json!({"type":"roof","points":[{"x":16,"y":24},{"x":24,"y":24}],"width":4,"roof_type":"hip","edge":"solid","ridge":"solid","hip":"solid"}),
            Some("solid"),
        ),
        item(
            "mask",
            json!({"type":"object","asset":"mask","position":{"x":32,"y":24},"size":{"x":4,"y":4},"rotation":0,"pivot":{"x":0.5,"y":0.5},"flip_x":false,"flip_y":false}),
            None,
        ),
    ];
    items[1].floor_holes = vec![rect(12., 4., 2., 2.)];
    items[5].water = Some(WaterStyle {
        deep_color: [50, 150, 150, 255],
        shallow_color: [150, 200, 180, 255],
        blend_distance: 1.,
        disable_border: false,
        smoothing: 0.6,
    });
    items[6].cave = Some(CaveStyle {
        colorable: true,
        ground_color: [255; 4],
        wall_color: [255; 4],
        blasts: vec![rect(44., 4., 2., 2.)],
    });
    items[6].border = Some("solid".into());
    let child = item(
        "child",
        json!({"type":"object","asset":"cutout","position":{"x":-2,"y":0},"size":{"x":2,"y":2},"rotation":0,"pivot":{"x":0.5,"y":0.5},"flip_x":false,"flip_y":false}),
        None,
    );
    let mut second = child.clone();
    second.translate(Point::new(4., 0.));
    let prefab =
        maploom::storage::prefabs::Prefab::capture("Pair".into(), vec![child, second]).unwrap();
    let mut instance = prefab.instance(Point::new(44., 24.), 1., 0.).unwrap();
    instance.id = "prefab".into();
    items.push(instance);
    let expected: HashSet<_> = items.iter().map(|i| i.id.clone()).collect();
    project.levels[0].layers[1].items = items;
    let mut renderer = SceneRenderer::default();
    let options = |known_meshes| SceneOptions {
        level: 0,
        grid: true,
        lights: true,
        revision: 0,
        known_meshes,
    };
    let scene = renderer
        .scene(&project, &library, options(HashSet::new()), &HashSet::new())
        .unwrap();
    let owners: HashSet<_> = scene.meshes.iter().filter_map(|m| m.item.clone()).collect();
    assert_eq!(owners, expected);
    assert!(scene.meshes.iter().any(|m| m.item.is_none())); // Background and grid never capture clicks.
    for mesh in &scene.meshes {
        assert_eq!(
            STANDARD.decode(&mesh.vertices).unwrap().len(),
            mesh.vertex_count * 48
        );
    }
    if let Some(path) = std::env::var_os("MAPLOOM_PICK_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec(&json!({"scene":scene,"project":project})).unwrap(),
        )
            .unwrap();
    }
    let cached = renderer
        .scene(
            &project,
            &library,
            options(scene.meshes.iter().map(|m| m.key.clone()).collect()),
            &scene.used_textures.iter().cloned().collect(),
        )
        .unwrap();
    assert_eq!(
        cached
            .meshes
            .iter()
            .filter_map(|m| m.item.clone())
            .collect::<HashSet<_>>(),
        expected
    );
    assert!(cached.meshes.iter().all(|m| m.vertices.is_empty()));
}
