use maploom::assets;
use std::io::{BufWriter, Cursor, Write};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const ENTRY_COUNT: u32 = 200_001;

#[test]
fn zip_accepts_more_than_two_hundred_thousand_members() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for index in 0..ENTRY_COUNT {
        writer
            .start_file(format!("entry-{index}"), options)
            .unwrap();
    }
    let mut archive = ZipArchive::new(writer.finish().unwrap()).unwrap();
    assets::validate_zip(&mut archive).unwrap();
    assert_eq!(archive.len(), ENTRY_COUNT as usize);
    assert_eq!(
        archive.by_index(ENTRY_COUNT as usize - 1).unwrap().name(),
        "entry-200000"
    );
}

#[test]
fn pck_versions_accept_more_than_two_hundred_thousand_members() {
    let directory = tempfile::tempdir().unwrap();
    for version in [1u32, 2] {
        let path = directory
            .path()
            .join(format!("large-v{version}.dungeondraft_pack"));
        let mut writer = BufWriter::new(std::fs::File::create(&path).unwrap());
        writer.write_all(b"GDPC").unwrap();
        for value in [version, 3, 4, 2] {
            writer.write_all(&value.to_le_bytes()).unwrap();
        }
        if version == 2 {
            writer.write_all(&0u32.to_le_bytes()).unwrap();
            writer.write_all(&0u64.to_le_bytes()).unwrap();
        }
        writer.write_all(&[0; 64]).unwrap();
        writer.write_all(&ENTRY_COUNT.to_le_bytes()).unwrap();
        for index in 0..ENTRY_COUNT {
            let name = format!("res://entry-{index}");
            writer
                .write_all(&(name.len() as u32).to_le_bytes())
                .unwrap();
            writer.write_all(name.as_bytes()).unwrap();
            // Empty entries need no payload; retain valid offsets and checksums.
            writer.write_all(&[0; 32]).unwrap();
            if version == 2 {
                writer.write_all(&0u32.to_le_bytes()).unwrap();
            }
        }
        writer.flush().unwrap();
        drop(writer);
        let entries = assets::pck_entries(&path).unwrap();
        assert_eq!(entries.len(), ENTRY_COUNT as usize);
        assert_eq!(entries.last().unwrap().0, "res://entry-200000");
    }
}

#[test]
fn tags_apply_references_after_the_former_count_limit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("item.png");
    image::RgbaImage::new(1, 1).save(&path).unwrap();
    let mut asset = assets::import_image(&path).unwrap();
    asset.meta.id = "res://textures/objects/item.png".into();
    let mut references = vec!["textures/objects/missing.png"; ENTRY_COUNT as usize];
    *references.last_mut().unwrap() = "textures/objects/item.png";
    let tags = serde_json::to_vec(&serde_json::json!({"tags": {"Colorable": references}})).unwrap();
    assets::apply_dd_tags(std::slice::from_mut(&mut asset), &tags).unwrap();
    assert!(asset.meta.tags.iter().any(|tag| tag == "Colorable"));
    assert_eq!(
        asset.meta.color_mode,
        maploom::document::ObjectColorMode::RedMask
    );
}
