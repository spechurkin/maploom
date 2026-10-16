use crate::{
    assets::{self, Asset, Library, MAX_JSON, Source, safe_member},
    document::*,
};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub struct Loaded {
    pub project: Project,
    pub library: Library,
    pub warnings: Vec<String>,
}
pub fn load(path: &Path) -> Result<Loaded> {
    let mut file = File::open(path)?;
    let mut signature = [0; 4];
    file.read_exact(&mut signature)?;
    if signature == *b"PK\x03\x04" {
        let mut zip = ZipArchive::new(File::open(path)?)?;
        assets::validate_zip(&mut zip)?;
        let mut project: Project =
            serde_json::from_slice(&assets::read_zip(&mut zip, "project.json", MAX_JSON)?)?;
        project.validate()?;
        project.migrate_terrain();
        project.validate()?;
        let mut library = Library::new();
        for (key, meta) in &project.assets {
            if project.external_assets.contains_key(key) {
                continue;
            }
            safe_member(&meta.path)?;
            zip.by_name(&meta.path)?;
            library.insert(
                key.clone(),
                Asset {
                    meta: meta.clone(),
                    source: Source::Zip {
                        path: path.to_path_buf(),
                        entry: meta.path.clone(),
                    },
                },
            );
        }
        Ok(Loaded {
            project,
            library,
            warnings: vec![],
        })
    } else {
        ensure!(
            file.metadata()?.len() <= MAX_JSON,
            "Project JSON exceeds 64 MiB"
        );
        let raw: Value = serde_json::from_reader(File::open(path)?)?;
        if raw.get("world").is_some() {
            return import_dungeondraft(raw, path);
        }
        let wrapped = raw.get("project");
        let mut project: Project = serde_json::from_value(wrapped.unwrap_or(&raw).clone())?;
        project.validate()?;
        project.migrate_terrain();
        project.validate()?;
        let mut library = Library::new();
        let mut warnings = vec![];
        for (key, meta) in &project.assets {
            if let Some(encoded) = raw["embedded"][key].as_str() {
                let bytes = STANDARD.decode(encoded)?;
                ensure!(
                    bytes.len() as u64 <= assets::MAX_ENTRY,
                    "Embedded image exceeds limit"
                );
                library.insert(
                    key.clone(),
                    Asset {
                        meta: meta.clone(),
                        source: Source::Bytes(Arc::new(bytes)),
                    },
                );
            } else if !project.external_assets.contains_key(key) {
                warnings.push(format!("Asset {key} is not embedded; load its fabric"));
            }
        }
        Ok(Loaded {
            project,
            library,
            warnings,
        })
    }
}
pub fn save(path: &Path, project: &Project, library: &Library) -> Result<()> {
    project.validate()?;
    let mut project = project.clone();
    project.migrate_terrain();
    project.sync_active_variation();
    project.validate()?;
    resolve_dd_assets(&mut project, library)?;
    let mut blobs = BTreeMap::new();
    let mut wanted: HashSet<String> = project.assets.keys().cloned().collect();
    wanted.extend(crate::catalog::used_assets(&project));
    for key in wanted {
        if let Some(asset) = library.get(&key) {
            if let Source::Catalog { id, .. } = &asset.source {
                project.external_assets.insert(key.clone(), id.clone());
                project
                    .assets
                    .entry(key)
                    .or_insert_with(|| asset.meta.clone());
                continue;
            }
            project.external_assets.remove(&key);
            let bytes = assets::verified_bytes(asset)?;
            let hash = assets::hash(&bytes);
            let mut meta = asset.meta.clone();
            let ext = image::guess_format(&bytes)?.extensions_str()[0];
            meta.path = format!("assets/{hash}.{ext}");
            meta.sha256 = hash;
            blobs.insert(meta.path.clone(), bytes);
            project.assets.insert(key, meta);
        }
    }
    ensure!(
        project
            .assets
            .iter()
            .all(|(id, a)| project.external_assets.contains_key(id) || blobs.contains_key(&a.path)),
        "Project has unavailable embedded assets; load the required fabric before saving"
    );
    ensure!(
        blobs.values().map(|b| b.len() as u64).sum::<u64>() <= assets::MAX_TOTAL,
        "Project assets exceed 2 GiB"
    );
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
    {
        let embedded: BTreeMap<_, _> = project
            .assets
            .iter()
            .filter_map(|(id, meta)| {
                blobs
                    .get(&meta.path)
                    .map(|b| (id.clone(), STANDARD.encode(b)))
            })
            .collect();
        let data = serde_json::to_vec_pretty(&json!({"project":project,"embedded":embedded}))?;
        ensure!(
            data.len() as u64 <= MAX_JSON,
            "Text project exceeds 64 MiB; use .mloom instead"
        );
        return assets::atomic_write(path, |f| {
            f.write_all(&data)?;
            Ok(())
        });
    }
    assets::atomic_write(path, |file| {
        let data = serde_json::to_vec_pretty(&project)?;
        ensure!(data.len() as u64 <= MAX_JSON, "Project JSON exceeds 64 MiB");
        let mut zip = ZipWriter::new(file);
        zip.start_file(
            "project.json",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated),
        )?;
        zip.write_all(&data)?;
        for (name, bytes) in &blobs {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )?;
            zip.write_all(bytes)?;
        }
        zip.finish()?;
        Ok(())
    })
}
pub fn resolve_dd_assets(project: &mut Project, library: &Library) -> Result<()> {
    let Some(scales) = project
        .extensions
        .get("dungeondraft_object_scales")
        .and_then(|v| v.as_object())
        .cloned()
    else {
        return Ok(());
    };
    let mut sizes = BTreeMap::new();
    let mut resolved = vec![];
    for level in project.all_levels_mut() {
        for layer in &mut level.layers {
            for item in &mut layer.items {
                if let ItemKind::Object { asset, size, .. } = &mut item.kind {
                    if let Some(scale) = scales.get(&item.id) {
                        if let Some(source) = library.get(asset) {
                            if !sizes.contains_key(asset) {
                                let bytes = assets::verified_bytes(source)?;
                                let image = assets::decode(&bytes)?;
                                sizes.insert(
                                    asset.clone(),
                                    Point::new(
                                        image.width() as f32 / 256.,
                                        image.height() as f32 / 256.,
                                    ),
                                );
                            }
                            let natural = sizes[asset];
                            let scale = dd_point(scale, 1.);
                            *size =
                                Point::new(natural.x * scale.x.abs(), natural.y * scale.y.abs());
                            resolved.push(item.id.clone());
                        }
                    }
                }
            }
        }
    }
    if let Some(v) = project
        .extensions
        .get_mut("dungeondraft_object_scales")
        .and_then(|v| v.as_object_mut())
    {
        for key in resolved {
            v.remove(&key);
        }
    }
    project.sync_active_variation();
    Ok(())
}
pub fn numbers(value: &Value) -> Vec<f32> {
    if let Some(s) = value.as_str() {
        let body = s
            .split_once('(')
            .map(|p| p.1.trim_end_matches(')'))
            .unwrap_or(s);
        body.split([',', ' ', '\t', '\n'])
            .filter_map(|s| s.parse::<f32>().ok())
            .collect()
    } else if let Some(a) = value.as_array() {
        a.iter()
            .filter_map(|v| v.as_f64().map(|n| n as f32))
            .collect()
    } else if value.is_object() {
        vec![number(value, "x", 0.), number(value, "y", 0.)]
    } else {
        vec![]
    }
}
pub fn dd_point(v: &Value, scale: f32) -> Point {
    let n = numbers(v);
    Point::new(
        n.first().copied().unwrap_or(0.) / scale,
        n.get(1).copied().unwrap_or(0.) / scale,
    )
}
pub(crate) fn dd_points(v: &Value) -> Vec<Point> {
    if let Some(a) = v.as_array() {
        if a.first().is_some_and(|v| v.is_object() || v.is_string()) {
            return a.iter().map(|v| dd_point(v, 256.)).collect();
        }
    }
    numbers(v)
        .chunks_exact(2)
        .map(|p| Point::new(p[0] / 256., p[1] / 256.))
        .collect()
}
fn number(v: &Value, key: &str, default: f32) -> f32 {
    v[key].as_f64().map(|n| n as f32).unwrap_or(default)
}
pub fn dd_color(v: &Value, default: Color) -> Color {
    let Some(s) = v.as_str() else { return default };
    let s = s.trim_start_matches('#');
    if s.len() != 8 {
        return default;
    }
    if let Ok(n) = u32::from_str_radix(s, 16) {
        [
            ((n >> 16) & 255) as u8,
            ((n >> 8) & 255) as u8,
            (n & 255) as u8,
            ((n >> 24) & 255) as u8,
        ]
    } else {
        default
    }
}
fn values(v: &Value) -> Vec<&Value> {
    if let Some(a) = v.as_array() {
        a.iter().collect()
    } else if let Some(o) = v.as_object() {
        o.values().collect()
    } else {
        vec![]
    }
}
fn portal(v: &Value) -> Item {
    let p = dd_point(&v["position"], 256.);
    let rotation = number(v, "rotation", 0.);
    let r = number(v, "radius", 128.) / 256.;
    let d = Point::new(rotation.cos() * r, rotation.sin() * r);
    Item::new(
        "Portal",
        ItemKind::Portal {
            bounds: [
                Point::new(p.x - d.x, p.y - d.y),
                Point::new(p.x + d.x, p.y + d.y),
            ],
            closed: v["closed"].as_bool().unwrap_or(true),
            color: [157, 113, 76, 255],
        },
    )
    .with_texture(v["texture"].as_str())
}
pub fn import_dungeondraft(raw: Value, path: &Path) -> Result<Loaded> {
    let world = &raw["world"];
    ensure!(world.is_object(), "Dungeondraft map has no world");
    let width = world["width"]
        .as_u64()
        .context("Dungeondraft map has no width")?;
    let height = world["height"]
        .as_u64()
        .context("Dungeondraft map has no height")?;
    ensure!(
        (1..=512).contains(&width) && (1..=512).contains(&height),
        "Unsupported DD map dimensions"
    );
    let mut project = Project {
        name: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        width: width as u32,
        height: height as u32,
        levels: vec![],
        background_asset: world["levels"]["0"]["terrain"]["texture_1"]
            .as_str()
            .map(str::to_owned),
        use_default_assets: raw["header"]["uses_default_assets"]
            .as_bool()
            .unwrap_or(true),
        ..Project::default()
    };
    let mut warnings = vec![];
    let mut scales = serde_json::Map::new();
    let levels = world["levels"]
        .as_object()
        .context("Dungeondraft levels must be an object")?;
    let mut levels: Vec<_> = levels.iter().collect();
    levels.sort_by_key(|(k, _)| k.parse::<i64>().unwrap_or(0));
    for (key, v) in levels {
        let mut level = Level::new(v["label"].as_str().unwrap_or(key));
        for shape in values(&v["shapes"]["polygons"]) {
            let points = dd_points(shape);
            if points.len() >= 3 {
                level.layers[0].items.push(
                    Item::new(
                        "DD floor",
                        ItemKind::Floor {
                            points,
                            color: [154, 149, 131, 255],
                        },
                    )
                    .with_texture(v["tiles"]["lookup"]["0"].as_str()),
                );
            }
        }
        for pattern in values(&v["patterns"]) {
            let points = dd_points(
                pattern
                    .get("points")
                    .or_else(|| pattern.get("polygon"))
                    .unwrap_or(&Value::Null),
            );
            if points.len() >= 3 {
                level.layers[0].items.push(
                    Item::new(
                        "DD pattern",
                        ItemKind::Floor {
                            points,
                            color: dd_color(&pattern["color"], [147, 135, 108, 255]),
                        },
                    )
                    .with_texture(pattern["texture"].as_str()),
                );
            }
        }
        for wall in values(&v["walls"]) {
            let mut points = dd_points(&wall["points"]);
            if wall["loop"].as_bool().unwrap_or(false)
                && points.len() > 2
                && points.first() != points.last()
            {
                points.push(points[0]);
            }
            if points.len() >= 2 {
                level.layers[1].items.push(
                    Item::new(
                        "DD wall",
                        ItemKind::Wall {
                            points,
                            width: 0.16,
                            color: dd_color(&wall["color"], [70, 72, 62, 255]),
                            blocks_sight: true,
                        },
                    )
                    .with_texture(wall["texture"].as_str()),
                );
            }
            for p in values(&wall["portals"]) {
                level.layers[1].items.push(portal(p));
            }
        }
        for p in values(&v["portals"]) {
            level.layers[1].items.push(portal(p));
        }
        for line in values(&v["paths"]) {
            let points = dd_points(&line["points"]);
            if points.len() >= 2 {
                level.layers[0].items.push(
                    Item::new(
                        "DD path",
                        ItemKind::Path {
                            points,
                            width: (number(line, "width", 128.) / 256.).clamp(0.1, 5.),
                            color: dd_color(&line["color"], [129, 115, 89, 255]),
                        },
                    )
                    .with_texture(line["texture"].as_str()),
                );
            }
        }
        let mut user_layers: BTreeMap<i64, Layer> = BTreeMap::new();
        for object in values(&v["objects"]) {
            let Some(asset) = object["texture"].as_str() else {
                continue;
            };
            let scale = if object.get("scale").is_some() {
                dd_point(&object["scale"], 1.)
            } else {
                Point::new(1., 1.)
            };
            let mut item = Item::new(
                Path::new(asset)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy(),
                ItemKind::Object {
                    asset: asset.into(),
                    position: dd_point(&object["position"], 256.),
                    size: Point::new(scale.x.abs().max(0.01), scale.y.abs().max(0.01)),
                    rotation: number(object, "rotation", 0.),
                    pivot: Point::new(0.5, 0.5),
                    flip_x: object["mirror"].as_bool().unwrap_or(false) || scale.x < 0.,
                    flip_y: scale.y < 0.,
                },
            );
            item.tint = dd_color(&object["custom_color"], [255; 4]);
            scales.insert(item.id.clone(), json!(scale));
            let index = object["layer"].as_i64().unwrap_or(100);
            user_layers
                .entry(index)
                .or_insert_with(|| {
                    Layer::new(
                        v["layers"][index.to_string()]
                            .as_str()
                            .unwrap_or("DD objects"),
                    )
                })
                .items
                .push(item);
        }
        level.layers.extend(user_layers.into_values());
        for light in values(&v["lights"]) {
            level.layers[2].items.push(Item::new(
                "DD light",
                ItemKind::Light {
                    position: dd_point(&light["position"], 256.),
                    range: number(light, "range", 3.).max(0.01),
                    intensity: number(light, "intensity", 1.).max(0.),
                    smoothing: crate::light::default_smoothing(),
                    color: dd_color(&light["color"], [255, 213, 137, 255]),
                    shadows: light["shadows"].as_bool().unwrap_or(true),
                },
            ));
        }
        for text in values(&v["texts"]) {
            level.layers[2].items.push(Item::new(
                "DD text",
                ItemKind::Text {
                    position: dd_point(&text["position"], 256.),
                    text: text["text"].as_str().unwrap_or("").into(),
                    size: (number(text, "size", 32.) / 256.).max(0.05),
                    color: dd_color(&text["color"], [255, 255, 255, 255]),
                },
            ));
        }
        if v["terrain"]["enabled"].as_bool() == Some(true)
            || v.get("cave").is_some()
            || v.get("materials").is_some()
            || v.get("roofs").is_some()
        {
            warnings.push(format!("{}: terrain blend maps, cave bitmaps, materials and roofs are preserved as source JSON; their original appearance is not rendered yet", level.name));
        }
        project.levels.push(level);
    }
    for pack in values(&raw["header"]["asset_manifest"]) {
        project.packs.push(PackRef {
            name: pack["name"].as_str().unwrap_or("DD pack").into(),
            path: pack["path"].as_str().unwrap_or("").into(),
        });
    }
    project.extensions.insert("dungeondraft_source".into(), raw);
    project
        .extensions
        .insert("dungeondraft_object_scales".into(), Value::Object(scales));
    project.validate()?;
    Ok(Loaded {
        project,
        library: Library::new(),
        warnings,
    })
}

// Door intervals are removed from wall LOS so an open portal can actually be traversed.
pub fn line_of_sight(level: &Level) -> Vec<Vec<Point>> {
    line_of_sight_with_boundaries(level, |_| None)
}

pub(crate) fn object_boundaries(
    level: &Level,
    library: &Library,
) -> BTreeMap<String, Vec<[Point; 2]>> {
    let mut boundaries = BTreeMap::new();
    for item in level
        .layers
        .iter()
        .filter(|l| l.visible)
        .flat_map(|l| &l.items)
    {
        if let ItemKind::Object { asset, .. } = &item.kind {
            if item.blocks_light && !boundaries.contains_key(asset) {
                if let Some(image) = library
                    .get(asset)
                    .and_then(|a| assets::decode_surface(a).ok())
                {
                    boundaries.insert(asset.clone(), assets::alpha_boundary(&image));
                }
            }
        }
    }
    boundaries
}

pub fn line_of_sight_with_library(level: &Level, library: &Library) -> Vec<Vec<Point>> {
    // Resolve prefab children before collecting the assets used by blockers.
    let mut level = level.clone();
    for layer in &mut level.layers {
        layer.items = layer
            .items
            .iter()
            .flat_map(crate::storage::prefabs::parts)
            .collect();
    }
    let boundaries = object_boundaries(&level, library);
    line_of_sight_with_boundaries(&level, |id| boundaries.get(id).map(Vec::as_slice))
}

pub(crate) fn line_of_sight_with_boundaries<'a>(
    level: &Level,
    boundary: impl Fn(&str) -> Option<&'a [[Point; 2]]>,
) -> Vec<Vec<Point>> {
    let expanded;
    let level = if level
        .layers
        .iter()
        .flat_map(|l| &l.items)
        .any(|i| i.prefab.is_some())
    {
        expanded = Level {
            layers: level
                .layers
                .iter()
                .map(|layer| {
                    let mut layer = layer.clone();
                    layer.items = layer
                        .items
                        .iter()
                        .flat_map(crate::storage::prefabs::parts)
                        .collect();
                    layer
                })
                .collect(),
            ..level.clone()
        };
        &expanded
    } else {
        level
    };
    let openings = crate::openings::Openings::new(level);
    let rooms = crate::caves::rooms(level);
    let mut output = vec![];
    for layer in level.layers.iter().filter(|l| l.visible) {
        for item in &layer.items {
            if let ItemKind::Object {
                asset,
                position,
                size,
                pivot,
                rotation,
                flip_x,
                flip_y,
            } = &item.kind
            {
                if item.blocks_light {
                    if let Some(edges) = boundary(asset) {
                        let (sin, cos) = rotation.sin_cos();
                        let at = |p: Point| {
                            let x = ((if *flip_x { 1. - p.x } else { p.x }) - pivot.x) * size.x;
                            let y = ((if *flip_y { 1. - p.y } else { p.y }) - pivot.y) * size.y;
                            Point::new(
                                position.x + x * cos - y * sin,
                                position.y + x * sin + y * cos,
                            )
                        };
                        output.extend(edges.iter().map(|[a, b]| vec![at(*a), at(*b)]));
                    } else {
                        // Missing artwork retains the visible placeholder footprint.
                        let corners =
                            crate::render::object_corners(*position, *size, *pivot, *rotation);
                        output.extend((0..4).map(|n| vec![corners[n], corners[(n + 1) % 4]]));
                    }
                }
                continue;
            }
            if item.cave.is_some() {
                output.extend(
                    crate::caves::boundary(item, &rooms)
                        .into_iter()
                        .map(Vec::from),
                );
                continue;
            }
            if let ItemKind::Path { points, .. } = &item.kind {
                if item.path_style.as_ref().is_some_and(|s| s.blocks_light) {
                    output.extend(
                        crate::paths::sample(points, item.path_style.as_ref().unwrap().smoothness)
                            .windows(2)
                            .map(|p| p.to_vec()),
                    );
                }
                continue;
            }
            let openings = openings.for_wall(&item.id);
            let lines = if matches!(
                item.kind,
                ItemKind::Wall {
                    blocks_sight: false,
                    ..
                }
            ) {
                vec![]
            } else {
                item.wall_segments()
            };
            for line in lines {
                for (lo, hi) in openings.spans(line) {
                    output.push(vec![
                        crate::openings::at(line, lo),
                        crate::openings::at(line, hi),
                    ]);
                }
            }
        }
    }
    output
}
pub fn dd2vtt(
    project: &Project,
    level: usize,
    pixels_per_grid: u32,
    png: &[u8],
    library: &Library,
) -> Value {
    let level = &project.levels[level];
    let items: Vec<_> = level
        .layers
        .iter()
        .filter(|l| l.visible)
        .flat_map(|l| &l.items)
        .collect();
    // DD2VTT `closed` means blocks light, not the open/closed state of a door.
    // Universal Battlemap Importer reads false as a window and true as a door.
    // Its "Openable Windows" setting can still give windows door controls;
    // the exporter cannot override that importer setting.
    let portals: Vec<_> = items.iter().filter_map(|i| {
        let ItemKind::Portal { bounds, .. } = &i.kind else { return None; };
        Some(json!({
            "position": {"x": (bounds[0].x + bounds[1].x) / 2., "y": (bounds[0].y + bounds[1].y) / 2.},
            "bounds": bounds,
            "rotation": (bounds[1].y - bounds[0].y).atan2(bounds[1].x - bounds[0].x),
            "closed": i.portal_type_with_asset(i.texture.as_ref().and_then(|id| {
                library.get(id).map(|asset| &asset.meta).or_else(|| project.assets.get(id))
            })) == PortalType::Door,
            "freestanding": false
        }))
    }).collect();
    let lights: Vec<_> = items.iter().filter_map(|i| if let ItemKind::Light { position, range, intensity, color, shadows, .. } = &i.kind { Some(json!({"position":position,"range":range,"intensity":intensity,"color":format!("{:02x}{:02x}{:02x}{:02x}",color[3],color[0],color[1],color[2]),"shadows":shadows})) } else { None }).collect();
    json!({"format":0.3,"resolution":{"map_origin":{"x":0,"y":0},"map_size":{"x":project.width,"y":project.height},"pixels_per_grid":pixels_per_grid},"line_of_sight":line_of_sight_with_library(level, library),"objects_line_of_sight":[],"portals":portals,"lights":lights,"environment":{"baked_lighting":false,"ambient_light":format!("ff{:02x}{:02x}{:02x}",project.ambient_light[0],project.ambient_light[1],project.ambient_light[2])},"image":STANDARD.encode(png)})
}
