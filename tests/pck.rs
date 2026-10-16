use maploom::assets;
use std::io::Write;

fn image_pack(path: &std::path::Path, entries: &[(&str, Vec<u8>)]) {
    image_pack_version(path, entries, 1);
}

fn image_pack_version(path: &std::path::Path, entries: &[(&str, Vec<u8>)], version: u32) {
    let mut file = std::fs::File::create(path).unwrap();
    file.write_all(b"GDPC").unwrap();
    for value in [version, 3, 4, 2] {
        file.write_all(&value.to_le_bytes()).unwrap();
    }
    let base = if version == 2 { 12u64 } else { 0 };
    if version == 2 {
        file.write_all(&0u32.to_le_bytes()).unwrap();
        file.write_all(&base.to_le_bytes()).unwrap();
    }
    file.write_all(&[0; 64]).unwrap();
    file.write_all(&(entries.len() as u32).to_le_bytes())
        .unwrap();
    let header_size = if version == 2 { 100 } else { 88 };
    let mut offset = header_size
        + entries
            .iter()
            .map(|(name, _)| 4 + name.len() + 32 + if version == 2 { 4 } else { 0 })
            .sum::<usize>() as u64
        - base;
    for (name, body) in entries {
        file.write_all(&(name.len() as u32).to_le_bytes()).unwrap();
        file.write_all(name.as_bytes()).unwrap();
        file.write_all(&offset.to_le_bytes()).unwrap();
        file.write_all(&(body.len() as u64).to_le_bytes()).unwrap();
        file.write_all(&[0; 16]).unwrap();
        if version == 2 {
            file.write_all(&0u32.to_le_bytes()).unwrap();
        }
        offset += body.len() as u64;
    }
    for (_, body) in entries {
        file.write_all(body).unwrap();
    }
}

#[test]
fn duplicate_pck_entries_keep_first_bytes_and_continue_loading_the_catalog() {
    use maploom::catalog;
    let dir = tempfile::tempdir().unwrap();
    let first = "res://textures/objects/first.png";
    let last = "res://textures/objects/last.png";
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::new(2, 3)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    for version in [1, 2] {
        let path = dir.path().join("duplicates.dungeondraft_pack");
        image_pack_version(
            &path,
            &[
                ("res://empty", vec![]),
                (first, png.clone()),
                (first, b"ignored duplicate".to_vec()),
                ("res://empty", vec![]),
                (first, b"another ignored duplicate".to_vec()),
                (last, png.clone()),
            ],
            version,
        );
        let entries = assets::pck_entries(&path).unwrap();
        assert_eq!(
            entries.iter().map(|e| e.0.as_str()).collect::<Vec<_>>(),
            ["res://empty", first, last]
        );
        let directory = catalog::directory(dir.path(), None).unwrap();
        let catalog = catalog::scan(&directory);
        assert!(catalog.warnings.is_empty(), "{:?}", catalog.warnings);
        assert_eq!(catalog.packs.len(), 1);
        assert_eq!(catalog.packs[0].assets.len(), 2);
        assert_eq!(catalog.library.len(), 2);
        for id in [first, last] {
            let bytes = assets::verified_bytes(&catalog.library[id]).unwrap();
            assert_eq!(bytes, png);
            assert_eq!(assets::decode(&bytes).unwrap().dimensions(), (2, 3));
        }
    }
}

#[test]
fn repeated_pck_metadata_and_tags_use_the_first_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metadata.dungeondraft_pack");
    for version in [1, 2] {
        image_pack_version(
            &path,
            &[
                (
                    "res://pack/pack.json",
                    br#"{"name":"First","id":"original"}"#.to_vec(),
                ),
                (
                    "res://pack/pack.json",
                    br#"{"name":"Duplicate","id":"wrong"}"#.to_vec(),
                ),
                (
                    "res://data/default.dungeondraft_tags",
                    br#"{"tags":{"Colorable":["textures/objects/item.png"]}}"#.to_vec(),
                ),
                (
                    "res://data/default.dungeondraft_tags",
                    br#"{"tags":{"Wrong":["textures/objects/item.png"]}}"#.to_vec(),
                ),
                ("res://textures/objects/item.png", b"lazy image".to_vec()),
            ],
            version,
        );
        let pack = assets::index_pack(&path).unwrap();
        assert!(pack.warnings.is_empty(), "{:?}", pack.warnings);
        assert_eq!(pack.name, "First");
        assert_eq!(pack.metadata.id, "original");
        assert_eq!(pack.assets.len(), 1);
        assert!(pack.assets[0].meta.tags.iter().any(|t| t == "Colorable"));
        assert!(!pack.assets[0].meta.tags.iter().any(|t| t == "Wrong"));
    }
}

#[test]
fn duplicate_pck_records_can_share_payload_but_cannot_hide_invalid_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.dungeondraft_pack");
    let name = "res://textures/objects/item.png";
    for version in [1, 2] {
        image_pack_version(
            &path,
            &[(name, b"first".to_vec()), (name, b"later".to_vec())],
            version,
        );
        let original = std::fs::read(&path).unwrap();
        let header = if version == 2 { 100 } else { 88 };
        let first_offset = header + 4 + name.len();
        let second_offset = first_offset + 4 + name.len() + 32 + if version == 2 { 4 } else { 0 };
        let offset =
            u64::from_le_bytes(original[first_offset..first_offset + 8].try_into().unwrap());
        let mut shared = original.clone();
        shared[second_offset..second_offset + 8].copy_from_slice(&offset.to_le_bytes());
        std::fs::write(&path, &shared).unwrap();
        let pack = assets::index_pack(&path).unwrap();
        assert_eq!(pack.assets.len(), 1);
        assert_eq!(pack.assets[0].source.read().unwrap(), b"first");

        for invalid in [offset - 1, offset + 1, original.len() as u64 + 1] {
            let mut bytes = original.clone();
            bytes[second_offset..second_offset + 8].copy_from_slice(&invalid.to_le_bytes());
            std::fs::write(&path, bytes).unwrap();
            assert!(
                assets::index_pack(&path).is_err(),
                "version {version}, offset {invalid}"
            );
        }
    }
}

#[test]
fn pack_image_headers_preserve_natural_path_proportions_without_decoding_pixels() {
    use maploom::document::*;
    let dir = tempfile::tempdir().unwrap();
    for format in [
        image::ImageFormat::Png,
        image::ImageFormat::Jpeg,
        image::ImageFormat::WebP,
        image::ImageFormat::Bmp,
        image::ImageFormat::Tiff,
        image::ImageFormat::Gif,
    ] {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            768,
            64,
            image::Rgb([60, 90, 120]),
        ));
        let mut bytes = std::io::Cursor::new(vec![]);
        image.write_to(&mut bytes, format).unwrap();
        let id = format!(
            "res://packs/test/textures/paths/strip.{}",
            format.extensions_str()[0]
        );
        let file = dir.path().join("paths.dungeondraft_pack");
        image_pack(
            &file,
            &[
                ("res://padding.bin", vec![0; 73]),
                (&id, bytes.into_inner()),
            ],
        );
        let pack = assets::index_pack(&file).unwrap();
        assert_eq!(pack.assets[0].meta.size, Point::new(3., 0.25), "{format:?}");
        let mut item = Item::new(
            "Path",
            ItemKind::Path {
                points: vec![Point::new(1., 1.), Point::new(13., 1.)],
                width: 0.5,
                color: [255; 4],
            },
        )
        .with_texture(Some(&id));
        item.texture_scale = 0.5 * pack.assets[0].meta.size.x / pack.assets[0].meta.size.y;
        // Twelve cells contain two full images, each retaining its 12:1 ratio.
        assert_eq!(maploom::paths::item_ribbon(&item).last().unwrap().uv.x, 2.);
    }
    // A PNG header still supplies dimensions with an invalid pixel payload.
    let mut bytes = std::io::Cursor::new(vec![]);
    image::RgbaImage::new(512, 64)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let bytes = bytes.into_inner()[..41].to_vec();
    assert!(assets::decode(&bytes).is_err());
    let file = dir.path().join("header.dungeondraft_pack");
    image_pack(&file, &[("res://textures/paths/header.png", bytes)]);
    assert_eq!(
        assets::index_pack(&file).unwrap().assets[0].meta.size,
        Point::new(2., 0.25)
    );
    // The next entry cannot complete a truncated image header.
    image_pack(
        &file,
        &[
            ("res://textures/paths/truncated.png", b"\x89PNG".to_vec()),
            ("res://next.bin", vec![0; 128]),
        ],
    );
    assert_eq!(
        assets::index_pack(&file).unwrap().assets[0].meta.size,
        Point::new(1., 1.)
    );
}

#[test]
fn pack_index_keeps_unrelated_images_lazy_and_measures_tag_defined_paths() {
    use maploom::document::Point;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("tagged.dungeondraft_pack");
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::new(768, 64)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    let png = png.into_inner();
    image_pack(
        &file,
        &[
            ("res://textures/objects/banner.png", png.clone()),
            ("res://textures/custom/ribbon.png", png),
            (
                "res://data/default.dungeondraft_tags",
                br#"{"tags":{"Paths":["textures/custom/ribbon.png"]}}"#.to_vec(),
            ),
        ],
    );
    let pack = assets::index_pack(&file).unwrap();
    let object = pack
        .assets
        .iter()
        .find(|a| assets::category(&a.meta) == "objects")
        .unwrap();
    let path = pack
        .assets
        .iter()
        .find(|a| assets::category(&a.meta) == "paths")
        .unwrap();
    assert_eq!(
        object.meta.size,
        Point::new(1., 1.),
        "unrelated images stay lazy"
    );
    assert_eq!(
        path.meta.size,
        Point::new(3., 0.25),
        "tag-defined paths keep their proportions"
    );
}

#[test]
fn legacy_pack_paths_repair_automatic_repeats_once_across_variations_and_roundtrip() {
    use maploom::{catalog, document::*, formats};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("paths.dungeondraft_pack");
    let id = "res://textures/paths/banner.png";
    let mut png = std::io::Cursor::new(vec![]);
    image::RgbaImage::new(768, 64)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    image_pack(&file, &[(id, png.into_inner())]);
    let directory = catalog::directory(dir.path(), None).unwrap();
    let indexed = catalog::scan(&directory);
    let mut project = Project {
        background_asset: None,
        use_default_assets: false,
        asset_catalogs: vec![CatalogRef {
            id: directory.id.clone(),
            name: directory.name.clone(),
            packs: None,
        }],
        ..Default::default()
    };
    let mut meta = indexed.library[id].meta.clone();
    meta.size = Point::new(1., 1.);
    project.assets.insert(id.into(), meta);
    let mut path = Item::new(
        "Automatic",
        ItemKind::Path {
            points: vec![Point::new(1., 1.), Point::new(13., 1.)],
            width: 0.5,
            color: [255; 4],
        },
    )
    .with_texture(Some(id));
    path.texture_scale = 0.5;
    project.levels[0].layers[1].items.push(path.clone());
    project.add_variation("Copy".into()).unwrap();
    let variation = project.active_variation.clone().unwrap();
    let mut manual = path.clone();
    manual.id = maploom::document::id();
    manual.texture_scale = 2.25;
    project.levels[0].layers[1].items.push(manual);
    let mut library = assets::Library::new();
    assert!(
        catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed)).is_empty()
    );
    assert_eq!(project.assets[id].size, Point::new(3., 0.25));
    assert_eq!(project.levels[0].layers[1].items[0].texture_scale, 6.);
    assert_eq!(project.levels[0].layers[1].items[1].texture_scale, 2.25);
    assert!(
        project
            .variations
            .iter()
            .filter(|v| v.id != variation)
            .all(|v| v.levels[0].layers[1].items[0].texture_scale == 6.)
    );
    let repaired = project.clone();
    catalog::resolve(&mut project, &mut library, std::slice::from_ref(&indexed));
    assert_eq!(project, repaired);
    for extension in ["mloom", "mloom.json"] {
        let saved = dir.path().join(format!("repaired.{extension}"));
        formats::save(&saved, &project, &library).unwrap();
        let mut loaded = formats::load(&saved).unwrap();
        catalog::resolve(
            &mut loaded.project,
            &mut loaded.library,
            std::slice::from_ref(&indexed),
        );
        assert_eq!(loaded.project.assets[id].size, Point::new(3., 0.25));
        assert_eq!(
            loaded.project.levels[0].layers[1].items[0].texture_scale,
            6.
        );
        assert_eq!(
            loaded.project.levels[0].layers[1].items[1].texture_scale,
            2.25
        );
    }
}

/// Read-only regression probe for the user's actual asset packs.
#[test]
#[ignore = "requires MAPLOOM_PATH_PACK; optionally MAPLOOM_PATH_PROOF for rendered evidence"]
fn local_pack_path_dimensions_match_decoded_images_and_render_at_natural_proportions() {
    use maploom::{document::*, render};
    let pack = assets::index_pack(&std::path::PathBuf::from(
        std::env::var_os("MAPLOOM_PATH_PACK").unwrap(),
    ))
    .unwrap();
    let paths: Vec<_> = pack
        .assets
        .iter()
        .filter(|a| assets::category(&a.meta) == "paths")
        .collect();
    assert!(!paths.is_empty());
    for asset in &paths {
        let image = assets::decode_surface(asset).unwrap();
        assert_eq!(
            asset.meta.size,
            Point::new(image.width() as f32 / 256., image.height() as f32 / 256.),
            "{}",
            asset.meta.id
        );
    }
    eprintln!(
        "{}: {} path textures retain their pixel proportions",
        pack.name,
        paths.len()
    );
    if let Some(output) = std::env::var_os("MAPLOOM_PATH_PROOF") {
        let output = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&output).unwrap();
        let asset_name = std::env::var("MAPLOOM_PATH_ASSET").ok();
        let asset = asset_name
            .as_ref()
            .map(|name| {
                paths
                    .iter()
                    .find(|a| a.meta.name == *name)
                    .expect("requested path asset")
            })
            .unwrap_or_else(|| {
                paths
                    .iter()
                    .max_by(|a, b| {
                        (a.meta.size.x / a.meta.size.y).total_cmp(&(b.meta.size.x / b.meta.size.y))
                    })
                    .unwrap()
            });
        let mut project = Project {
            background_asset: None,
            use_default_assets: false,
            width: 20,
            height: 10,
            ..Default::default()
        };
        for (n, scale) in [0.75, 0.75 * asset.meta.size.x / asset.meta.size.y]
            .into_iter()
            .enumerate()
        {
            let y = 2. + n as f32 * 4.;
            let mut path = Item::new(
                if n == 0 { "Before" } else { "Fixed" },
                ItemKind::Path {
                    points: vec![
                        Point::new(1., y),
                        Point::new(8., y),
                        Point::new(12., y + 1.),
                        Point::new(18., y),
                    ],
                    width: 0.75,
                    color: [255; 4],
                },
            )
            .with_texture(Some(&asset.meta.id));
            path.texture_scale = scale;
            path.path_style = Some(maploom::paths::PathStyle {
                smoothness: 1.,
                ..Default::default()
            });
            project.levels[0].layers[1].items.push(path);
        }
        let library = [(asset.meta.id.clone(), (*asset).clone())]
            .into_iter()
            .collect();
        let scene = maploom::gpu_scene::SceneRenderer::default()
            .scene(
                &project,
                &library,
                maploom::gpu_scene::SceneOptions {
                    level: 0,
                    grid: true,
                    lights: false,
                    revision: 1,
                    known_meshes: Default::default(),
                },
                &Default::default(),
            )
            .unwrap();
        std::fs::write(
            output.join("scene.json"),
            serde_json::to_vec(&scene).unwrap(),
        )
        .unwrap();
        std::fs::write(
            output.join("project.json"),
            serde_json::to_vec(&project).unwrap(),
        )
        .unwrap();
        let options = render::ExportOptions {
            level: 0,
            pixels_per_grid: 80,
            grid: true,
            lights: false,
            jpeg_quality: 90,
        };
        let (image, _) = render::rasterize(&project, &library, options).unwrap();
        image.save(output.join("path-proportions.png")).unwrap();
        std::fs::write(
            output.join("path-proportions.svg"),
            render::svg(&project, &library, options).unwrap(),
        )
        .unwrap();
        maploom::formats::save(&output.join("path-proportions.mloom"), &project, &library).unwrap();
        eprintln!(
            "{}: size {:?}, repeat {} cells",
            asset.meta.name, asset.meta.size, project.levels[0].layers[1].items[1].texture_scale
        );
    }
}
fn pck(path: &std::path::Path, version: u32, offset_delta: i64) {
    let name = b"res://textures/objects/test.png";
    let body = b"image data";
    let header = 4 + 4 + 12 + 64 + 4 + 4 + name.len() + 8 + 8 + 16;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"GDPC").unwrap();
    f.write_all(&version.to_le_bytes()).unwrap();
    for value in [3u32, 4, 2] {
        f.write_all(&value.to_le_bytes()).unwrap();
    }
    f.write_all(&[0; 64]).unwrap();
    f.write_all(&1u32.to_le_bytes()).unwrap();
    f.write_all(&(name.len() as u32).to_le_bytes()).unwrap();
    f.write_all(name).unwrap();
    f.write_all(&((header as i64 + offset_delta) as u64).to_le_bytes())
        .unwrap();
    f.write_all(&(body.len() as u64).to_le_bytes()).unwrap();
    f.write_all(&[0; 16]).unwrap();
    f.write_all(body).unwrap();
}
#[test]
fn pck_indexes_without_decoding_images() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.dungeondraft_pack");
    pck(&path, 1, 0);
    let pack = assets::index_pack(&path).unwrap();
    assert_eq!(pack.assets.len(), 1);
    assert_eq!(pack.assets[0].source.read().unwrap(), b"image data");
    assert!(assets::decode(&pack.assets[0].source.read().unwrap()).is_err());
}
#[test]
fn pck_out_of_bounds_offsets_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.dungeondraft_pack");
    pck(&path, 1, 200);
    assert!(assets::index_pack(&path).is_err());
    pck(&path, 1, -5);
    assert!(assets::index_pack(&path).is_err());
}
#[test]
fn future_pck_version_is_not_misread() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.dungeondraft_pack");
    pck(&path, 42, 0);
    assert!(assets::index_pack(&path).is_err());
}
#[test]
fn parallel_indexing_returns_every_success_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut paths = vec![];
    for i in 0..20 {
        let path = dir.path().join(format!("{i}.dungeondraft_pack"));
        pck(&path, 1, if i % 4 == 0 { 100 } else { 0 });
        paths.push(path);
    }
    let results = assets::index_packs(&paths);
    assert_eq!(results.len(), 20);
    assert_eq!(results.iter().filter(|(_, r)| r.is_ok()).count(), 15);
    assert_eq!(results.iter().filter(|(_, r)| r.is_err()).count(), 5);
    for (path, r) in results {
        if let Ok(pack) = r {
            assert_eq!(pack.path, path);
            assert_eq!(pack.assets[0].source.read().unwrap(), b"image data");
        }
    }
}
#[test]
fn pack_tag_metadata_accepts_godot_trailing_commas_without_changing_strings() {
    let dir = tempfile::tempdir().unwrap();
    for pack in ["first", "second"] {
        let path = dir.path().join(format!("{pack}.dungeondraft_pack"));
        let image_name = format!("res://packs/{pack}/textures/objects/item.png");
        let tag_name = format!("res://packs/{pack}/data/default.dungeondraft_tags");
        let tags = br#"{"tags":{"Colorable":["textures/objects/item.png",],"Label , } and \"quote\" ":["res://textures/objects/item.png",],},}"#;
        let entries = [
            (image_name.as_bytes(), b"image data".as_slice()),
            (tag_name.as_bytes(), tags.as_slice()),
        ];
        let header_size = 88
            + entries
                .iter()
                .map(|(name, _)| 4 + name.len() + 32)
                .sum::<usize>();
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(b"GDPC").unwrap();
        for value in [1u32, 3, 4, 2] {
            file.write_all(&value.to_le_bytes()).unwrap();
        }
        file.write_all(&[0; 64]).unwrap();
        file.write_all(&2u32.to_le_bytes()).unwrap();
        let mut offset = header_size as u64;
        for (name, body) in entries {
            file.write_all(&(name.len() as u32).to_le_bytes()).unwrap();
            file.write_all(name).unwrap();
            file.write_all(&offset.to_le_bytes()).unwrap();
            file.write_all(&(body.len() as u64).to_le_bytes()).unwrap();
            file.write_all(&[0; 16]).unwrap();
            offset += body.len() as u64;
        }
        for (_, body) in entries {
            file.write_all(body).unwrap();
        }
        drop(file);
        let indexed = assets::index_pack(&path).unwrap();
        assert!(indexed.warnings.is_empty(), "{:?}", indexed.warnings);
        assert_eq!(
            indexed.assets[0].meta.object_color_mode(),
            maploom::document::ObjectColorMode::RedMask
        );
        assert!(
            indexed.assets[0]
                .meta
                .tags
                .iter()
                .any(|t| t == "Label , } and \"quote\" ")
        );
    }
}
