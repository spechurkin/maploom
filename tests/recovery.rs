use maploom::{
    assets::{self, Library, Source},
    document::*,
    formats,
    storage::recovery,
};
use std::{
    fs,
    time::{Duration, SystemTime},
};

fn project(name: &str) -> Project {
    Project {
        name: name.into(),
        background_asset: None,
        ..Project::default()
    }
}

#[test]
fn five_versions_rotate_independently_for_same_named_projects_and_survive_renaming() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = project("Одна карта");
    let mut b = project("Одна карта");
    let library = Library::new();
    for n in 1..=8 {
        a.width = n;
        recovery::save(dir.path(), &a, &library).unwrap();
        if n <= 3 {
            b.width = 100 + n;
            recovery::save(dir.path(), &b, &library).unwrap();
        }
    }
    let latest_a = recovery::latest(dir.path(), Some(&a.id)).unwrap().unwrap();
    let latest_b = recovery::latest(dir.path(), Some(&b.id)).unwrap().unwrap();
    assert_ne!(latest_a.path.parent(), latest_b.path.parent());
    assert_eq!(
        fs::read_dir(latest_a.path.parent().unwrap())
            .unwrap()
            .count(),
        5
    );
    assert_eq!(
        fs::read_dir(latest_b.path.parent().unwrap())
            .unwrap()
            .count(),
        3
    );
    for n in 1..=5 {
        let path = latest_a
            .path
            .with_file_name(format!("Одна карта-recovery-{n}.mloom"));
        assert_eq!(formats::load(&path).unwrap().project.width, 9 - n);
    }
    assert_eq!(formats::load(&latest_b.path).unwrap().project.width, 103);
    a.name = "Новое имя".into();
    a.width = 9;
    let path = recovery::save(dir.path(), &a, &library).unwrap();
    assert_eq!(path.file_name().unwrap(), "Новое имя-recovery-1.mloom");
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 5);
    for n in 1..=5 {
        assert_eq!(
            formats::load(&path.with_file_name(format!("Новое имя-recovery-{n}.mloom")))
                .unwrap()
                .project
                .width,
            10 - n
        );
    }
}

#[test]
fn startup_selects_newest_project_and_falls_back_from_corrupt_archives() {
    let dir = tempfile::tempdir().unwrap();
    let a = project("Earlier");
    let b = project("Later");
    let library = Library::new();
    let old = recovery::save(dir.path(), &a, &library).unwrap();
    let recent = recovery::save(dir.path(), &b, &library).unwrap();
    fs::File::options()
        .write(true)
        .open(&old)
        .unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(60))
        .unwrap();
    assert_eq!(
        recovery::latest(dir.path(), None).unwrap().unwrap().path,
        recent
    );
    recovery::save(dir.path(), &b, &library).unwrap();
    fs::write(&recent, b"broken archive").unwrap();
    assert!(recovery::latest(dir.path(), Some(&b.id))
        .unwrap()
        .unwrap()
        .path
        .ends_with("Later-recovery-2.mloom"));
    assert_eq!(
        recovery::latest(dir.path(), None).unwrap().unwrap().name,
        "Later"
    );
    // Rotating an old version must not make it look newer than another project.
    let fallback = recent.with_file_name("Later-recovery-2.mloom");
    let old_time = SystemTime::now() - Duration::from_secs(120);
    fs::File::options()
        .write(true)
        .open(&fallback)
        .unwrap()
        .set_modified(old_time)
        .unwrap();
    recovery::save(dir.path(), &b, &library).unwrap();
    assert_eq!(
        fs::metadata(recent.with_file_name("Later-recovery-3.mloom"))
            .unwrap()
            .modified()
            .unwrap(),
        old_time
    );
    fs::write(&recent, b"broken").unwrap();
    fs::write(
        recent.with_file_name("Later-recovery-2.mloom"),
        b"also broken",
    )
        .unwrap();
    assert_eq!(
        recovery::latest(dir.path(), None).unwrap().unwrap().path,
        old
    );
    assert!(recovery::latest(dir.path(), Some("missing"))
        .unwrap()
        .is_none());
}

#[test]
fn invalid_names_and_imported_ids_stay_inside_local_project_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project("../Карта: <ночь>?*|\\");
    p.id = "../../outside".into();
    let path = recovery::save(dir.path(), &p, &Library::new()).unwrap();
    assert_eq!(path.parent().unwrap().parent().unwrap(), dir.path());
    assert!(!path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .contains(['/', '\\', ':', '<', '>', '?', '*', '|']));
    assert_eq!(formats::load(&path).unwrap().project, p);
}

#[test]
fn failed_serialization_keeps_all_previous_backups_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let mut p = project("Safe");
    let path = recovery::save(dir.path(), &p, &Library::new()).unwrap();
    let before = fs::read(&path).unwrap();
    p.width = 0;
    assert!(recovery::save(dir.path(), &p, &Library::new()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn restored_assets_survive_rotation_of_the_archive_they_came_from() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("image.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([100, 20, 30, 255]))
        .save(&image)
        .unwrap();
    let asset = assets::import_image(&image).unwrap();
    let mut p = project("Embedded");
    p.assets.insert(asset.meta.id.clone(), asset.meta.clone());
    let library = Library::from([(asset.meta.id.clone(), asset.clone())]);
    let path = recovery::save(dir.path(), &p, &library).unwrap();
    let restored = recovery::load(&path).unwrap();
    assert!(matches!(
        restored.library[&asset.meta.id].source,
        Source::Bytes(_)
    ));
    p.assets.clear();
    for _ in 0..6 {
        recovery::save(dir.path(), &p, &Library::new()).unwrap();
    }
    let restored_path = dir.path().join("restored.mloom");
    formats::save(&restored_path, &restored.project, &restored.library).unwrap();
    let loaded = formats::load(&restored_path).unwrap();
    assert_eq!(
        assets::verified_bytes(&loaded.library[&asset.meta.id]).unwrap(),
        fs::read(image).unwrap()
    );
}
