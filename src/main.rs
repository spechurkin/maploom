#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use anyhow::{Context, Result, bail};
use maploom::{
    assets,
    document::Project,
    formats,
    render::{self, ExportOptions},
};
use std::path::{Path, PathBuf};

fn cli(args: &[String]) -> Result<bool> {
    let Some(command) = args.first() else {
        return Ok(false);
    };
    match command.as_str() {
        "theme-directory" => {
            println!("{}", maploom::platform::themes::directory()?.display());
            Ok(true)
        }
        "export-prefabs" => {
            let input = Path::new(
                args.get(1)
                    .context("export-prefabs requires a fabric/prefab file")?,
            );
            let output = Path::new(
                args.get(2)
                    .context("export-prefabs requires output.mloom_prefab")?,
            );
            let pack = assets::index_pack(input)?;
            anyhow::ensure!(!pack.prefabs.is_empty(), "This file contains no prefabs");
            let library = pack
                .assets
                .into_iter()
                .map(|a| (a.meta.id.clone(), a))
                .collect();
            let (prefabs, images) = maploom::storage::prefabs::portable(&pack.prefabs, &library)?;
            assets::save_prefab_file(output, &pack.name, &images, &prefabs)?;
            println!(
                "Exported {} prefabs with {} embedded images",
                prefabs.len(),
                images.len()
            );
            Ok(true)
        }
        "import-dd-prefabs" => {
            use maploom::storage::{dd_prefabs, prefabs};
            let pck = Path::new(
                args.get(1)
                    .context("import-dd-prefabs requires Dungeondraft.pck")?,
            );
            let output = Path::new(
                args.get(2)
                    .context("import-dd-prefabs requires an existing fabric")?,
            );
            let pack = assets::index_pack(output)?;
            let mut library: assets::Library = pack
                .assets
                .into_iter()
                .map(|a| (a.meta.id.clone(), a))
                .collect();
            for asset in assets::dd_defaults(pck)?.assets {
                library.entry(asset.meta.id.clone()).or_insert(asset);
            }
            let definitions = dd_prefabs::read(pck)?;
            anyhow::ensure!(!definitions.is_empty(), "No Dungeondraft prefabs found");
            let mut converted = vec![];
            for definition in &definitions {
                let mut prefab = dd_prefabs::parse(definition, &mut library)?;
                prefabs::add_preview(&mut prefab, &mut library)?;
                println!("{}: {} components", prefab.name, prefab.items.len());
                converted.push(prefab);
            }
            assets::save_fabric_with_prefabs(
                output,
                &pack.name,
                &pack.metadata.author,
                &library.into_values().collect::<Vec<_>>(),
                &converted,
            )?;
            if let Some(path) = args.get(3) {
                let bytes = serde_json::to_vec_pretty(&definitions)?;
                assets::atomic_write(Path::new(path), |file| {
                    use std::io::Write;
                    file.write_all(&bytes)?;
                    Ok(())
                })?;
            }
            println!("Imported {} original Dungeondraft prefabs", converted.len());
            Ok(true)
        }
        "import-dd-assets" => {
            let pck = Path::new(
                args.get(1)
                    .context("import-dd-assets requires Dungeondraft.pck")?,
            );
            let dest = Path::new(
                args.get(2)
                    .context("import-dd-assets requires output fabric")?,
            );
            let pack = assets::dd_defaults(pck)?;
            assets::save_fabric(
                dest,
                &pack.name,
                "Local Dungeondraft installation",
                &pack.assets,
            )?;
            println!("Imported {} textures", pack.assets.len());
            Ok(true)
        }
        "--help" | "-h" => {
            println!(
                "Maploom {} — Tauri VTT map studio\n\nmaploom [project.mloom | map.dungeondraft_map]\nmaploom --demo\nmaploom inspect <project-or-pack>\nmaploom convert <project> <output.mloom-or-json>\nmaploom export <project> <output.png|webp|jpeg|svg|dd2vtt> [pixels-per-grid] [pack ...]\nmaploom pack <image-folder> <output.mloom_fabric>\nmaploom export-prefabs <fabric> <output.mloom_prefab>\nmaploom import-dd-prefabs <Dungeondraft.pck> <fabric> [definitions.json]\nmaploom gpu-info\nmaploom --smoke-test <output-directory>\n\nWGPU_BACKEND=dx12|vulkan|metal selects a GPU backend. Assets remain local.",
                env!("CARGO_PKG_VERSION")
            );
            Ok(true)
        }
        "gpu-info" => {
            let gpu = render::create_gpu()?;
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"name":gpu.info.name,"backend":format!("{:?}",gpu.info.backend),"device_type":format!("{:?}",gpu.info.device_type),"max_texture_size":gpu.device.limits().max_texture_dimension_2d})
                )?
            );
            Ok(true)
        }
        "inspect" => {
            let path = Path::new(args.get(1).context("inspect requires a file")?);
            if path.extension().is_some_and(|e| {
                matches!(
                    e.to_string_lossy().to_lowercase().as_str(),
                    "mloom_fabric" | "mloon_fabric" | "dungeondraft_pack" | "mloom_prefab"
                )
            }) {
                let pack = assets::index_pack(path)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"name":pack.name,"assets":pack.assets.len(),"prefabs":pack.prefabs.iter().map(|p| serde_json::json!({"name":p.name,"components":p.items.len()})).collect::<Vec<_>>(),"warnings":pack.warnings,
                            "metadata":{"id":pack.metadata.id,"author":pack.metadata.author,"version":pack.metadata.version,
                            "description":pack.metadata.description,"website":pack.metadata.website,"has_logo":pack.metadata.logo.is_some()}})
                    )?
                );
            } else {
                let loaded = formats::load(path)?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &serde_json::json!({"name":loaded.project.name,"width":loaded.project.width,"height":loaded.project.height,"levels":loaded.project.levels.iter().map(|l|serde_json::json!({"name":l.name,"layers":l.layers.len(),"items":l.layers.iter().map(|l|l.items.len()).sum::<usize>()})).collect::<Vec<_>>(),"embedded_assets":loaded.library.len(),"warnings":loaded.warnings})
                    )?
                );
            }
            Ok(true)
        }
        "bench-packs" => {
            let dir = Path::new(args.get(1).context("bench-packs requires a directory")?);
            let mut paths: Vec<_> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_pack"))
                })
                .collect();
            paths.sort();
            let threads = std::thread::available_parallelism()
                .map(|n| n.get().clamp(2, 8))
                .unwrap_or(4);
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()?;
            let started = std::time::Instant::now();
            let results = pool.install(|| assets::index_packs(&paths));
            let elapsed = started.elapsed();
            let count = results
                .iter()
                .filter_map(|(_, r)| r.as_ref().ok())
                .map(|p| p.assets.len())
                .sum::<usize>();
            let errors: Vec<_> = results
                .iter()
                .filter_map(|(p, r)| r.as_ref().err().map(|e| format!("{}: {e:#}", p.display())))
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({"packs":paths.len(),"assets":count,"threads":threads,"index_ms":elapsed.as_secs_f64()*1000.,"archive_bytes":paths.iter().filter_map(|p|std::fs::metadata(p).ok()).map(|m|m.len()).sum::<u64>(),"images_decoded":0,"errors":errors})
                )?
            );
            anyhow::ensure!(errors.is_empty(), "Pack indexing failures");
            Ok(true)
        }
        "convert" => {
            let source = Path::new(args.get(1).context("convert requires input")?);
            let dest = Path::new(args.get(2).context("convert requires output")?);
            let mut loaded = formats::load(source)?;
            assets::load_defaults(&mut loaded.library)?;
            maploom::catalog::load_configured(
                &mut loaded,
                source
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_map")),
            );
            formats::save(dest, &loaded.project, &loaded.library)?;
            for warning in loaded.warnings {
                eprintln!("{warning}");
            }
            Ok(true)
        }
        "export" => {
            let source = Path::new(args.get(1).context("export requires input")?);
            let dest = Path::new(args.get(2).context("export requires output")?);
            let mut loaded = formats::load(source)?;
            assets::load_defaults(&mut loaded.library)?;
            maploom::catalog::load_configured(
                &mut loaded,
                source
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_map")),
            );
            let ppg = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(100);
            let paths: Vec<_> = args.iter().skip(4).map(PathBuf::from).collect();
            for (_, r) in assets::index_packs(&paths) {
                for asset in r?.assets {
                    loaded.library.entry(asset.meta.id.clone()).or_insert(asset);
                }
            }
            println!(
                "{}",
                render::export(
                    dest,
                    &loaded.project,
                    &loaded.library,
                    ExportOptions {
                        level: 0,
                        pixels_per_grid: ppg,
                        grid: false,
                        lights: false,
                        jpeg_quality: 92
                    }
                )?
            );
            Ok(true)
        }
        "pack" => {
            let source = Path::new(args.get(1).context("pack requires image folder")?);
            let dest = Path::new(args.get(2).context("pack requires output")?);
            let mut paths = vec![];
            collect_images(source, &mut paths)?;
            use rayon::prelude::*;
            let list = paths
                .par_iter()
                .map(|p| assets::import_image(p))
                .collect::<Result<Vec<_>>>()?;
            let mut unique = std::collections::BTreeMap::new();
            for asset in list {
                unique.entry(asset.meta.id.clone()).or_insert(asset);
            }
            assets::save_fabric(
                dest,
                source
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .as_ref(),
                "",
                &unique.into_values().collect::<Vec<_>>(),
            )?;
            Ok(true)
        }
        "--smoke-test" => {
            smoke(Path::new(
                args.get(1)
                    .context("--smoke-test requires output directory")?,
            ))?;
            Ok(true)
        }
        _ => Ok(false),
    }
}
fn collect_images(path: &Path, images: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            collect_images(&entry.path(), images)?
        } else if assets::is_image(&entry.path().to_string_lossy()) {
            images.push(entry.path())
        }
    }
    Ok(())
}
fn smoke(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let project = Project::demo();
    let mut library = assets::Library::new();
    assets::load_defaults(&mut library)?;
    let options = ExportOptions {
        level: 0,
        pixels_per_grid: 32,
        grid: false,
        lights: false,
        jpeg_quality: 90,
    };
    formats::save(&dir.join("demo.mloom"), &project, &library)?;
    let loaded = formats::load(&dir.join("demo.mloom"))?;
    if loaded.project.levels != project.levels {
        bail!("Project roundtrip mismatch")
    }
    let mut evidence = vec![];
    for ext in ["png", "webp", "jpeg", "svg", "dd2vtt"] {
        let path = dir.join(format!("demo.{ext}"));
        let gpu = render::export(&path, &project, &library, options)?;
        evidence.push(
            serde_json::json!({"format":ext,"size":std::fs::metadata(&path)?.len(),"renderer":gpu}),
        );
    }
    let raw: serde_json::Value =
        serde_json::from_reader(std::fs::File::open(dir.join("demo.dd2vtt"))?)?;
    let portal_count = project.levels[0]
        .layers
        .iter()
        .flat_map(|l| &l.items)
        .filter(|i| matches!(i.kind, maploom::document::ItemKind::Portal { .. }))
        .count();
    anyhow::ensure!(
        raw["portals"]
            .as_array()
            .is_some_and(|p| p.len() == portal_count),
        "Portal missing from VTT export"
    );
    anyhow::ensure!(
        raw["lights"].as_array().is_some_and(|p| p.len() == 3),
        "Lights missing from VTT export"
    );
    assets::atomic_write(&dir.join("smoke.json"), |f| {
        use std::io::Write;
        f.write_all(&serde_json::to_vec_pretty(
            &serde_json::json!({"project_roundtrip":true,"exports":evidence}),
        )?)?;
        Ok(())
    })?;
    println!("{}", serde_json::to_string_pretty(&evidence)?);
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if cli(&args)? {
        return Ok(());
    }
    let demo = args.iter().any(|s| s == "--demo");
    let path = args
        .first()
        .filter(|s| !s.starts_with('-'))
        .map(PathBuf::from);
    #[cfg(feature = "desktop")]
    {
        maploom::desktop::run(demo, path)?;
        Ok(())
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (demo, path);
        bail!("Desktop UI requires the desktop feature. Run npm run build first.");
    }
}
fn main() {
    if let Err(e) = run() {
        eprintln!("Maploom: {e:#}");
        if std::env::args().len() == 1 {
            rfd::MessageDialog::new()
                .set_title("Maploom")
                .set_description(format!("Could not start Maploom: {e:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1);
    }
}
