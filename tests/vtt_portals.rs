use maploom::{
    assets::{self, Library, Source},
    document::*,
    formats,
};
use serde_json::json;
use std::{path::Path, sync::Arc};

fn portal(texture: &str, closed: bool) -> Item {
    Item::new(
        "Renamed opening",
        ItemKind::Portal {
            bounds: [Point::new(2., 0.), Point::new(3., 0.)],
            closed,
            color: [255; 4],
        },
    )
    .with_texture(Some(texture))
}

fn export(items: Vec<Item>, library: &Library) -> serde_json::Value {
    let mut project = Project::default();
    project.levels[0].layers[1].items = items;
    formats::dd2vtt(&project, 0, 100, b"PNG", library)
}

#[test]
fn sanctuary_demo_exports_nine_windows_and_one_door() {
    let project = Project::demo();
    let vtt = formats::dd2vtt(&project, 0, 100, b"PNG", &Library::new());
    let portals = vtt["portals"].as_array().unwrap();
    assert_eq!(portals.len(), 10);
    assert_eq!(portals.iter().filter(|p| p["closed"] == false).count(), 9);
    assert_eq!(portals.iter().filter(|p| p["closed"] == true).count(), 1);
}

#[test]
fn legacy_windows_from_different_pack_naming_styles_never_export_as_doors() {
    // Representative names observed in installed packs, including renamed map items.
    let names = [
        "basicbuilding_window.png",
        "window_2_1.png",
        "BlueLinedLongWindow.webp",
        "Window_Glass_01.png",
        "AP00_PORTAL_barred_glass_window1.png",
        "Workshop-Portals_0000s_0005_Windows-07-copie.png",
        "airship_CellWindowS.webp",
        "Crystal Window Golden.png",
        "TC_Castle Walls Black Thin Window 01_3x2.webp",
        "TWindowNarrow01.png",
        "Window 1.webp",
        "Window_Wooden_1x1_01_A_Ashen.webp",
    ];
    let items = names
        .iter()
        .map(|name| portal(&format!("res://packs/id/textures/portals/{name}"), true))
        .collect();
    let vtt = export(items, &Library::new());
    assert_eq!(vtt["portals"].as_array().unwrap().len(), names.len());
    for window in vtt["portals"].as_array().unwrap() {
        assert_eq!(window["closed"], false);
        assert_eq!(window["bounds"][0], json!({"x":2.,"y":0.}));
    }
    // A directory name alone must not turn a door into a window.
    assert_eq!(
        export(
            vec![portal("C:/Windows/doors/door_01.png", true)],
            &Library::new()
        )["portals"][0]["closed"],
        true
    );
}

#[test]
fn explicit_type_survives_serialization_and_overrides_legacy_flags_and_names() {
    let mut window = portal("custom:opaque-id", true);
    window.portal_type = Some(PortalType::Window);
    let mut door = portal("window_01.png", false);
    door.portal_type = Some(PortalType::Door);
    let serialized = serde_json::to_vec(&vec![window, door]).unwrap();
    let items = serde_json::from_slice(&serialized).unwrap();
    let vtt = export(items, &Library::new());
    assert_eq!(vtt["portals"][0]["closed"], false);
    assert_eq!(vtt["portals"][1]["closed"], true);
}

#[test]
fn manual_window_type_survives_native_project_save_load_and_file_export() {
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255; 4]))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let bytes = png.into_inner();
    let id = "custom:/portals/WindowDoor.png";
    let asset = assets::Asset {
        meta: AssetMeta {
            id: id.into(),
            name: "WindowDoor".into(),
            path: "WindowDoor.png".into(),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: assets::hash(&bytes),
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(bytes)),
    };
    let library = Library::from([(id.into(), asset)]);
    let mut project = Project {
        width: 4,
        height: 4,
        background_asset: None,
        use_default_assets: false,
        ..Project::default()
    };
    let mut item = portal(id, true);
    item.portal_type = Some(PortalType::Window);
    project.levels[0].layers[1].items.push(item);
    project.levels[0].layers[1].items.push(portal(id, false));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("manual-window.mloom");
    formats::save(&path, &project, &library).unwrap();
    let loaded = formats::load(&path).unwrap();
    assert_eq!(
        loaded.project.levels[0].layers[1].items[0].portal_type,
        Some(PortalType::Window)
    );
    assert_eq!(
        formats::dd2vtt(&loaded.project, 0, 100, b"PNG", &loaded.library)["portals"][0]["closed"],
        false
    );
    let output = temp.path().join("manual-window.dd2vtt");
    maploom::render::export(
        &output,
        &loaded.project,
        &loaded.library,
        maploom::render::ExportOptions {
            level: 0,
            pixels_per_grid: 16,
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )
    .unwrap();
    let vtt: serde_json::Value = serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
    assert_eq!(vtt["portals"][0]["closed"], false);
    assert_eq!(vtt["portals"][1]["closed"], true);
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(vtt["image"].as_str().unwrap())
        .unwrap();
    assert_eq!(image::load_from_memory(&png).unwrap().width(), 64);
}

#[test]
fn mixed_window_door_names_are_doors_unless_manually_overridden() {
    for name in [
        "WindowDoor.png",
        "window_door_01.webp",
        "WINDOWDOOR.png",
        "Окно-дверь.png",
    ] {
        let mut item = portal(name, false);
        assert_eq!(
            export(vec![item.clone()], &Library::new())["portals"][0]["closed"],
            true,
            "{name}"
        );
        item.portal_type = Some(PortalType::Window);
        assert_eq!(
            export(vec![item], &Library::new())["portals"][0]["closed"],
            false
        );
    }
}

#[test]
fn window_tags_are_used_even_with_opaque_asset_names() {
    let mut library = Library::new();
    let mut asset = assets::Asset {
        meta: AssetMeta {
            id: "res://packs/custom/textures/portals/abc123.png".into(),
            name: "abc123".into(),
            path: "abc123.png".into(),
            tags: vec![],
            size: Point::new(1., 1.),
            pivot: Point::new(0.5, 0.5),
            sha256: String::new(),
            color_mode: ObjectColorMode::None,
        },
        source: Source::Bytes(Arc::new(vec![])),
    };
    assets::apply_dd_tags(
        std::slice::from_mut(&mut asset),
        br#"{"tags":{"[Pack] Windows":["textures/portals/abc123.png"]}}"#,
    )
    .unwrap();
    assert_eq!(asset.meta.portal_type(), PortalType::Window);
    library.insert(asset.meta.id.clone(), asset.clone());
    let item = portal(&asset.meta.id, true);
    assert_eq!(
        export(vec![item.clone()], &library)["portals"][0]["closed"],
        false
    );
    let mut project = Project::default();
    project.assets.insert(asset.meta.id.clone(), asset.meta);
    project.levels[0].layers[1].items.push(item);
    assert_eq!(
        formats::dd2vtt(&project, 0, 100, b"PNG", &Library::new())["portals"][0]["closed"],
        false
    );
}

#[test]
fn imported_transparent_portals_and_old_window_textures_export_as_windows() {
    let raw = json!({"world":{"width":8,"height":8,"levels":{"0":{"portals":[
        {"position":"Vector2(512, 256)","texture":"opaque.png","closed":false},
        {"position":"Vector2(768, 256)","texture":"TWindowNarrow.png","closed":true},
        {"position":"Vector2(1024, 256)","texture":"door.png","closed":true}
    ]}}}});
    let loaded = formats::import_dungeondraft(raw, Path::new("import.dungeondraft_map")).unwrap();
    let vtt = formats::dd2vtt(&loaded.project, 0, 100, b"PNG", &Library::new());
    let closed: Vec<_> = vtt["portals"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["closed"].as_bool().unwrap())
        .collect();
    assert_eq!(closed, [false, false, true]);
}

#[test]
fn window_openings_cut_los_and_respect_level_and_layer_visibility() {
    let mut project = Project::default();
    project.levels[0].layers[0].items.push(Item::new(
        "Wall",
        ItemKind::Wall {
            points: vec![Point::new(0., 0.), Point::new(5., 0.)],
            width: 0.1,
            color: [255; 4],
            blocks_sight: true,
        },
    ));
    project.levels[0].layers[1]
        .items
        .push(portal("window.png", true));
    let mut other = Level::new("Other floor");
    other.layers[1].items.push(portal("door.png", true));
    project.levels.push(other);
    let vtt = formats::dd2vtt(&project, 0, 100, b"PNG", &Library::new());
    assert_eq!(vtt["portals"].as_array().unwrap().len(), 1);
    assert_eq!(vtt["portals"][0]["closed"], false);
    assert_eq!(
        vtt["line_of_sight"],
        json!([[{"x":0.,"y":0.},{"x":2.,"y":0.}], [{"x":3.,"y":0.},{"x":5.,"y":0.}]])
    );
    project.levels[0].layers[1].visible = false;
    let vtt = formats::dd2vtt(&project, 0, 100, b"PNG", &Library::new());
    assert_eq!(vtt["portals"], json!([]));
    assert_eq!(vtt["line_of_sight"].as_array().unwrap().len(), 1);
}
