use maploom::{
    assets::{self, Library, Source},
    document::*,
    formats,
    storage::prefabs::{self, Prefab, Store},
};

fn object(asset: &str, x: f32, y: f32) -> Item {
    Item::new(
        "Object",
        ItemKind::Object {
            asset: asset.into(),
            position: Point::new(x, y),
            size: Point::new(2., 1.),
            rotation: 0.,
            pivot: Point::new(0.25, 0.5),
            flip_x: true,
            flip_y: false,
        },
    )
}

#[test]
fn combined_instance_selects_by_parts_and_moves_duplicates_and_deletes_as_one_item() {
    use maploom::{editing, render};
    use std::collections::HashSet;
    let prefab = Prefab::capture(
        "Pair".into(),
        vec![object("a", -4., 0.), object("b", 4., 0.)],
    )
    .unwrap();
    let mut item = prefab.instance(Point::new(10., 10.), 1., 0.).unwrap();
    assert!(render::hit(&item, Point::new(6., 10.), 0.01));
    assert!(render::hit(&item, Point::new(14., 10.), 0.01));
    assert!(!render::hit(&item, Point::new(10., 10.), 0.01));
    let local = item.prefab.clone();
    let ItemKind::Object {
        size,
        rotation,
        flip_x,
        ..
    } = &mut item.kind
    else {
        panic!()
    };
    size.x *= 2.;
    size.y *= 2.;
    *rotation = std::f32::consts::FRAC_PI_2;
    *flip_x = true;
    let parts = prefabs::parts(&item);
    let ItemKind::Object {
        position: a,
        pivot,
        flip_x,
        ..
    } = parts[0].kind
    else {
        panic!()
    };
    let ItemKind::Object { position: b, .. } = parts[1].kind else {
        panic!()
    };
    assert!((b.y - a.y + 16.).abs() < 0.0001);
    assert_eq!(pivot.x, 0.75);
    assert!(!flip_x);
    assert_eq!(item.prefab, local);
    let mut project = Project {
        background_asset: None,
        ..Project::default()
    };
    let ids = HashSet::from([item.id.clone()]);
    project.levels[0].layers[1].items.push(item);
    editing::move_items(&mut project, 0, &ids, Point::new(2., 3.));
    project.validate().unwrap();
    assert_eq!(project.levels[0].layers[1].items[0].prefab, local);
    let copied = editing::duplicate_items(&mut project, 0, &ids);
    assert_eq!(copied.len(), 1);
    assert_eq!(project.levels[0].layers[1].items.len(), 2);
    assert_eq!(project.levels[0].layers[1].items[1].prefab, local);
    let first = prefabs::parts(&project.levels[0].layers[1].items[0]);
    let second = prefabs::parts(&project.levels[0].layers[1].items[1]);
    assert_ne!(first[0].id, second[0].id);
    editing::delete_items(&mut project, 0, &ids);
    assert_eq!(project.levels[0].layers[1].items.len(), 1);
    assert_eq!(project.levels[0].layers[1].items[0].id, copied[0]);
    let saved = serde_json::to_vec(&project).unwrap();
    let loaded: Project = serde_json::from_slice(&saved).unwrap();
    loaded.validate().unwrap();
    assert_eq!(loaded, project);
    let captured = Prefab::capture(
        "Composed again".into(),
        vec![
            loaded.levels[0].layers[1].items[0].clone(),
            object("c", 1., 1.),
        ],
    )
    .unwrap();
    assert_eq!(captured.items.len(), 3);
    assert!(captured.items.iter().all(|i| i.prefab.is_none()));
}

#[test]
fn combined_dd_prefab_renders_original_components_in_svg_and_keeps_child_light_blockers() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let library = pack
        .assets
        .into_iter()
        .map(|a| (a.meta.id.clone(), a))
        .collect();
    let prefab = pack.prefabs.iter().find(|p| p.name == "Well").unwrap();
    let mut combined = Project {
        background_asset: None,
        ..Project::default()
    };
    combined.levels[0].layers[1].items =
        vec![prefab.instance(Point::new(10., 10.), 1.5, 0.3).unwrap()];
    let mut expected = combined.clone();
    expected.levels[0].layers[1].items = prefab.place(Point::new(10., 10.), 1.5, 0.3).unwrap();
    let options = maploom::render::ExportOptions {
        level: 0,
        pixels_per_grid: 64,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    assert_eq!(
        maploom::render::svg(&combined, &library, options).unwrap(),
        maploom::render::svg(&expected, &library, options).unwrap()
    );
    let mut renderer = maploom::gpu_scene::SceneRenderer::default();
    let scene_options = || maploom::gpu_scene::SceneOptions {
        level: 0,
        grid: false,
        lights: false,
        revision: 1,
        known_meshes: Default::default(),
    };
    let a = renderer
        .scene(&combined, &library, scene_options(), &Default::default())
        .unwrap();
    let b = renderer
        .scene(&expected, &library, scene_options(), &Default::default())
        .unwrap();
    // Picking owns the combined parts as one instance; separate placements
    // retain independent owners and may therefore use more mesh batches.
    let owner = &combined.levels[0].layers[1].items[0].id;
    assert!(
        a.meshes
            .iter()
            .filter_map(|m| m.item.as_ref())
            .all(|id| id == owner)
    );
    assert!(b.meshes.iter().filter_map(|m| m.item.as_ref()).all(|id| {
        expected.levels[0].layers[1]
            .items
            .iter()
            .any(|i| &i.id == id)
    }));
    let triangles = |scene: &maploom::gpu_scene::GpuScene| {
        use base64::{Engine, engine::general_purpose::STANDARD};
        scene
            .meshes
            .iter()
            .flat_map(|mesh| {
                let vertices = STANDARD.decode(&mesh.vertices).unwrap();
                let indices = STANDARD.decode(&mesh.indices).unwrap();
                indices
                    .chunks_exact(12)
                    .map(|triangle| {
                        let data: Vec<u8> = triangle
                            .chunks_exact(4)
                            .flat_map(|index| {
                                let index = u32::from_le_bytes(index.try_into().unwrap()) as usize;
                                vertices[index * 48..(index + 1) * 48].iter().copied()
                            })
                            .collect();
                        (mesh.texture.clone(), mesh.mode, mesh.clip, data)
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(triangles(&a), triangles(&b));
    let mut solid = object("a", -4., 0.);
    solid.blocks_light = true;
    let prefab = Prefab::capture("Occluder".into(), vec![solid, object("b", 4., 0.)]).unwrap();
    combined.levels[0].layers[1].items =
        vec![prefab.instance(Point::new(10., 10.), 1., 0.).unwrap()];
    assert_eq!(formats::line_of_sight(&combined.levels[0]).len(), 4);
    assert!(
        maploom::light::Blockers::new(&combined, 0, &library)
            .visibility(Point::new(5., 5.), 10.)
            .is_some()
    );
}

#[test]
fn captures_relative_layout_and_places_editable_copies_with_one_global_transform() {
    let mut first = object("chair", 8., 9.);
    first.tint = [50, 100, 150, 200];
    first.blocks_light = true;
    first.locked = true;
    first.wall_attachment = Some(WallAttachment {
        wall: "old-map-wall".into(),
        segment: 1,
        t: 0.5,
        length: 2.,
    });
    let second = object("table", 12., 9.);
    let original = vec![first.clone(), second];
    let prefab = Prefab::capture("  Study  ".into(), original.clone()).unwrap();
    assert_eq!(prefab.name, "Study");
    assert_eq!(original[0], first, "capturing must never mutate the map");
    let placed = prefab
        .place(Point::new(20., 20.), 2., std::f32::consts::FRAC_PI_2)
        .unwrap();
    assert_ne!(placed[0].id, original[0].id);
    assert_ne!(
        placed[0].id,
        prefab.place(Point::new(20., 20.), 2., 0.).unwrap()[0].id
    );
    assert_eq!(placed[0].tint, first.tint);
    assert!(placed[0].blocks_light);
    assert!(!placed[0].locked);
    assert!(placed[0].wall_attachment.is_none());
    let ItemKind::Object {
        position: a,
        size,
        rotation,
        pivot,
        flip_x,
        ..
    } = placed[0].kind
    else {
        panic!()
    };
    let ItemKind::Object { position: b, .. } = placed[1].kind else {
        panic!()
    };
    assert!((a.x - b.x).abs() < 0.0001);
    assert!((b.y - a.y - 8.).abs() < 0.0001);
    assert_eq!(size, Point::new(4., 2.));
    assert_eq!(pivot, Point::new(0.25, 0.5));
    assert!(flip_x);
    assert_eq!(rotation, std::f32::consts::FRAC_PI_2);
}

#[test]
fn rejects_single_objects_mixed_selections_invalid_transforms_and_missing_images() {
    assert!(Prefab::capture("Single".into(), vec![object("a", 0., 0.)]).is_err());
    let nonobject = Item::new(
        "Light",
        ItemKind::Light {
            position: Point::new(0., 0.),
            range: 1.,
            intensity: 1.,
            color: [255; 4],
            shadows: true,
            smoothing: 1.,
        },
    );
    assert!(Prefab::capture("Mixed".into(), vec![object("a", 0., 0.), nonobject]).is_err());
    let prefab = Prefab::capture(
        "Valid".into(),
        vec![object("a", 0., 0.), object("a", 1., 0.)],
    )
    .unwrap();
    assert!(prefab.place(Point::new(0., 0.), f32::NAN, 0.).is_err());
    assert!(prefab.place(Point::new(0., 0.), 0., 0.).is_err());
    assert!(prefabs::portable(&[prefab], &Library::new()).is_err());
}

#[test]
fn portable_archive_survives_removed_sources_relocation_and_personal_library_edits() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("chair.png");
    image::RgbaImage::from_pixel(16, 8, image::Rgba([210, 60, 30, 255]))
        .save(&source)
        .unwrap();
    let mut image = assets::import_image(&source).unwrap();
    image.meta.id = "catalog:old/chair".into();
    image.source = Source::File(source.clone());
    image.meta.color_mode = ObjectColorMode::RedMask;
    let prefab = Prefab::capture(
        "Chairs".into(),
        vec![
            object(&image.meta.id, 0., 0.),
            object(&image.meta.id, 3., 0.),
        ],
    )
    .unwrap();
    let library = Library::from([(image.meta.id.clone(), image)]);
    let (prefabs, images) = prefabs::portable(&[prefab], &library).unwrap();
    assert_eq!(images.len(), 1, "shared image is embedded once");
    assert!(images[0].meta.id.starts_with("prefab-asset:"));
    assert_eq!(images[0].meta.color_mode, ObjectColorMode::RedMask);
    let expected = assets::verified_bytes(&images[0]).unwrap();
    let export = temp.path().join("export.mloom_prefab");
    assets::save_prefab_file(&export, "Chairs", &images, &prefabs).unwrap();
    std::fs::remove_file(source).unwrap();
    let imported = temp.path().join("another-computer.mloom_prefab");
    std::fs::rename(export, &imported).unwrap();
    let mut store = Store::load(&imported).unwrap();
    assert_eq!(store.personal, prefabs);
    let image_id = images[0].meta.id.clone();
    assert_eq!(
        assets::verified_bytes(&store.assets[&image_id]).unwrap(),
        expected
    );
    let (again, _) = prefabs::portable(&store.personal, &store.assets).unwrap();
    assert_eq!(
        again, store.personal,
        "re-export must preserve stable image identities"
    );
    let mut project = Project {
        background_asset: None,
        ..Project::default()
    };
    project.levels[0].layers[1].items = vec![
        store.personal[0]
            .instance(Point::new(5., 5.), 1., 0.)
            .unwrap(),
    ];
    let map = temp.path().join("placed.mloom");
    formats::save(&map, &project, &store.assets).unwrap();
    store.personal.clear();
    store.save(&imported).unwrap();
    assert!(Store::load(&imported).unwrap().personal.is_empty());
    assert_eq!(
        assets::verified_bytes(&store.assets[&image_id]).unwrap(),
        expected,
        "editing the library cannot invalidate live images"
    );
    let loaded = formats::load(&map).unwrap();
    assert_eq!(loaded.project.levels[0].layers[1].items.len(), 1);
    assert_eq!(
        loaded.project.levels[0].layers[1].items[0]
            .prefab
            .as_ref()
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        assets::verified_bytes(&loaded.library[&image_id]).unwrap(),
        expected
    );
}

#[test]
fn defaults_include_real_compositions_and_old_image_only_packs_remain_supported() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let recipes: serde_json::Value =
        serde_json::from_str(include_str!("../resources/default-prefabs.json")).unwrap();
    assert_eq!(pack.prefabs.len(), recipes.as_array().unwrap().len());
    assert_eq!(pack.prefabs.len(), 15);
    prefabs::validate(&pack.prefabs, &pack.assets).unwrap();
    for (prefab, recipe) in pack.prefabs.iter().zip(recipes.as_array().unwrap()) {
        assert_eq!(prefab.name, recipe["name"].as_str().unwrap());
        assert_eq!(
            prefab
                .items
                .iter()
                .filter(|i| matches!(i.kind, ItemKind::Object { .. }))
                .count(),
            recipe["data"]["objects"].as_array().unwrap().len()
        );
        assert!(prefab.preview.is_some());
    }
    let temp = tempfile::tempdir().unwrap();
    let old = temp.path().join("images.mloom_fabric");
    assets::save_fabric(&old, "Images", "", &pack.assets[..1]).unwrap();
    assert!(assets::index_pack(&old).unwrap().prefabs.is_empty());
}

#[test]
fn exported_prefabs_are_discovered_in_normal_asset_catalogs() {
    let pack = assets::index_pack(&assets::default_fabric_path().unwrap()).unwrap();
    let library = pack
        .assets
        .into_iter()
        .map(|a| (a.meta.id.clone(), a))
        .collect();
    let (prefabs, images) = prefabs::portable(&pack.prefabs[..1], &library).unwrap();
    let temp = tempfile::tempdir().unwrap();
    assets::save_fabric_with_prefabs(
        &temp.path().join("camp.mloom_fabric"),
        "Camp",
        "",
        &images,
        &prefabs,
    )
    .unwrap();
    let directory = maploom::catalog::directory(temp.path(), None).unwrap();
    let catalog = maploom::catalog::scan(&directory);
    assert!(catalog.warnings.is_empty(), "{:?}", catalog.warnings);
    assert_eq!(catalog.packs.len(), 1);
    assert_eq!(catalog.packs[0].prefabs, prefabs);
}
