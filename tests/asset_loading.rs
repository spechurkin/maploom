use maploom::{assets, catalog, formats, preferences::Preferences};
use std::{path::PathBuf, time::Instant};

/// Read-only check against an explicitly supplied local project and settings file.
#[test]
#[ignore = "requires MAPLOOM_PROBE_PROJECT and MAPLOOM_PROBE_SETTINGS"]
fn selected_catalog_images_load() {
    let project_path = PathBuf::from(std::env::var_os("MAPLOOM_PROBE_PROJECT").unwrap());
    let settings_path = PathBuf::from(std::env::var_os("MAPLOOM_PROBE_SETTINGS").unwrap());
    let settings: Preferences =
        serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    let mut loaded = formats::load(&project_path).unwrap();
    let started = Instant::now();
    assets::load_defaults(&mut loaded.library).unwrap();
    let catalogs: Vec<_> = settings
        .asset_directories
        .iter()
        .filter(|dir| loaded.project.asset_catalogs.iter().any(|r| r.id == dir.id))
        .map(catalog::scan)
        .collect();
    let warnings = catalog::resolve(&mut loaded.project, &mut loaded.library, &catalogs);
    eprintln!(
        "{} images, {} packs, indexed in {:?}, {} warnings",
        loaded.library.len(),
        catalogs.iter().map(|c| c.packs.len()).sum::<usize>(),
        started.elapsed(),
        warnings.len()
    );
    let started = Instant::now();
    let clone = loaded.library.clone();
    eprintln!("whole-library clone: {:?}", started.elapsed());
    drop(clone);
    let mut failures = vec![];
    for category in [
        "tilesets", "objects", "walls", "terrain", "paths", "portals",
    ] {
        let started = Instant::now();
        let mut count = 0;
        for asset in loaded
            .library
            .values()
            .filter(|a| assets::category(&a.meta) == category)
            .take(48)
        {
            if let Err(error) = assets::decode_surface(asset) {
                failures.push(format!("{}: {error:#}", asset.meta.id));
            }
            count += 1;
        }
        eprintln!("{category}: {count} decoded in {:?}", started.elapsed());
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
