//! DD's prefab JSON stores absolute scene positions plus local path/shape transforms.
//! Contract verified against Level.LoadPrefab, Objects.LoadObject, Pathway.Load and
//! PatternShapes.LoadShape in the locally installed Dungeondraft 3.4.2 Godot bundle.
use crate::{
    assets::{self, Asset, Library, Source},
    document::*,
    formats,
    paths::{PathStyle, Transition},
    storage::prefabs::{self, Prefab},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{io::Cursor, path::Path, sync::Arc};

pub const BORDER: &str = "res://textures/borders/default_border.png";
pub const WHITE: &str = "prefab-asset:solid-white";

#[derive(Serialize, Deserialize)]
pub struct Definition {
    pub path: String,
    pub name: String,
    pub data: Value,
}

pub fn read(pck: &Path) -> Result<Vec<Definition>> {
    assets::pck_entries(pck)?
        .into_iter()
        .filter(|(name, _, _)| name.ends_with(".dungeondraft_prefab"))
        .map(|(path, offset, size)| {
            ensure!(size <= assets::MAX_JSON, "Prefab JSON exceeds limit");
            let bytes = Source::Pck {
                path: pck.into(),
                offset,
                size,
            }
            .read()?;
            let name = Path::new(&path)
                .file_stem()
                .context("Missing prefab name")?
                .to_string_lossy()
                .into_owned();
            Ok(Definition {
                name,
                path,
                data: serde_json::from_slice(&bytes)?,
            })
        })
        .collect()
}

fn number(value: &Value, key: &str, fallback: f32) -> f32 {
    value[key].as_f64().map(|n| n as f32).unwrap_or(fallback)
}
fn scale(value: &Value) -> Point {
    value
        .get("scale")
        .map(|v| formats::dd_point(v, 1.))
        .unwrap_or(Point::new(1., 1.))
}
fn transform(p: Point, value: &Value, rotation: &str) -> Point {
    let offset = formats::dd_point(&value["position"], 256.);
    let scale = scale(value);
    let (sin, cos) = number(value, rotation, 0.).sin_cos();
    Point::new(
        offset.x + p.x * scale.x * cos - p.y * scale.y * sin,
        offset.y + p.x * scale.x * sin + p.y * scale.y * cos,
    )
}

pub fn solid(library: &mut Library) -> Result<()> {
    if library.contains_key(WHITE) {
        return Ok(());
    }
    let mut bytes = Cursor::new(vec![]);
    image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4]))
        .write_to(&mut bytes, image::ImageFormat::Png)?;
    let bytes = bytes.into_inner();
    library.insert(
        WHITE.into(),
        Asset {
            meta: AssetMeta {
                id: WHITE.into(),
                name: "Prefab solid fill".into(),
                path: "images/prefab-white.png".into(),
                tags: vec![],
                size: Point::new(1., 1.),
                pivot: Point::new(0.5, 0.5),
                sha256: assets::hash(&bytes),
                color_mode: ObjectColorMode::Tint,
            },
            source: Source::Bytes(Arc::new(bytes)),
        },
    );
    Ok(())
}

/// DD uses Chaikin corner cutting, with iteration count based on edit-point count.
fn smooth_dd(points: &[Point], smoothness: f32, looped: bool) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let iterations = if points.len() > 100 {
        1
    } else if points.len() > 30 {
        2
    } else if points.len() > 10 {
        3
    } else {
        4
    };
    let cut = 0.05 + (smoothness * 0.5).clamp(0., 1.) * 0.4;
    let mut points = points.to_vec();
    if points.len() == 2 {
        points.insert(
            1,
            Point::new(
                (points[0].x + points[1].x) / 2.,
                (points[0].y + points[1].y) / 2.,
            ),
        );
    }
    for _ in 0..iterations {
        let mut next = vec![];
        if !looped {
            next.push(points[0]);
        }
        for n in 0..if looped {
            points.len()
        } else {
            points.len() - 1
        } {
            let a = points[n];
            let b = points[(n + 1) % points.len()];
            next.push(Point::new(
                a.x * (1. - cut) + b.x * cut,
                a.y * (1. - cut) + b.y * cut,
            ));
            next.push(Point::new(
                a.x * cut + b.x * (1. - cut),
                a.y * cut + b.y * (1. - cut),
            ));
        }
        if !looped {
            next.push(*points.last().unwrap());
        }
        points = next;
    }
    points
}

fn dd_profile(points: &[Point], value: &Value) -> crate::paths::PathProfile {
    let mut widths = vec![1.; points.len()];
    let count = if points.len() < 100 {
        points.len() / 3
    } else {
        50
    };
    let ease = |t: f32| {
        if t < 0.5 {
            (t * 2.).powi(2) / 2.
        } else {
            1. - ((1. - t) * 2.).powi(2) / 2.
        }
    };
    if count > 0 {
        for n in 0..count {
            let factor = ease(n as f32 / count as f32);
            if value["grow"].as_bool().unwrap_or(false) {
                widths[n] = factor;
            }
            if value["shrink"].as_bool().unwrap_or(false) {
                widths[points.len() - 1 - n] = factor;
            }
        }
    }
    let total: f32 = points.windows(2).map(|p| p[0].distance(p[1])).sum();
    let mut travelled = 0.;
    let alpha = points
        .iter()
        .enumerate()
        .map(|(n, _)| {
            if n > 0 {
                travelled += points[n - 1].distance(points[n]);
            }
            let t = travelled / total.max(1e-7);
            let start = if value["fade_in"].as_bool().unwrap_or(false) {
                (t / 0.1).clamp(0., 1.)
            } else {
                1.
            };
            let end = if value["fade_out"].as_bool().unwrap_or(false) {
                ((1. - t) / 0.1).clamp(0., 1.)
            } else {
                1.
            };
            start * end
        })
        .collect();
    crate::paths::PathProfile { widths, alpha }
}

pub fn parse(definition: &Definition, library: &mut Library) -> Result<Prefab> {
    let value = &definition.data;
    ensure!(value.is_object(), "Invalid Dungeondraft prefab");
    for (key, value) in value.as_object().unwrap() {
        ensure!(
            ["objects", "pathways", "pattern_shapes"].contains(&key.as_str())
                || value.as_array().is_some_and(|a| a.is_empty()),
            "Unsupported Dungeondraft prefab component: {key}"
        );
    }
    let mut components = vec![];
    for object in value["objects"].as_array().into_iter().flatten() {
        let asset = object["texture"]
            .as_str()
            .context("Prefab object has no texture")?;
        let meta = &library
            .get(asset)
            .with_context(|| format!("Missing prefab asset: {asset}"))?
            .meta;
        let scale = scale(object);
        let mut item = Item::new(
            &meta.name,
            ItemKind::Object {
                asset: asset.into(),
                position: formats::dd_point(&object["position"], 256.),
                size: Point::new(meta.size.x * scale.x.abs(), meta.size.y * scale.y.abs()),
                rotation: number(object, "rotation", 0.),
                pivot: Point::new(0.5, 0.5),
                flip_x: object["mirror"].as_bool().unwrap_or(scale.x < 0.),
                flip_y: scale.y < 0.,
            },
        );
        item.tint = formats::dd_color(&object["custom_color"], [255; 4]);
        components.push((number(object, "layer", 100.) as i32, 2, item));
    }
    for shape in value["pattern_shapes"].as_array().into_iter().flatten() {
        // DD uses its Null icon for an untextured, solid-colored polygon.
        ensure!(
            shape.get("texture").is_none(),
            "Textured DD prefab shapes are not supported"
        );
        solid(library)?;
        let points: Vec<_> = formats::dd_points(&shape["points"])
            .into_iter()
            .map(|p| transform(p, shape, "shape_rotation"))
            .collect();
        let mut item = Item::new(
            "DD pattern shape",
            ItemKind::Floor {
                points: points.clone(),
                color: [255; 4],
            },
        )
        .with_texture(Some(WHITE));
        item.tint = formats::dd_color(&shape["color"], [255; 4]);
        item.floor_outline = Some(false);
        let layer = number(shape, "layer", 100.) as i32;
        components.push((layer, 0, item));
        if shape["outline"].as_bool().unwrap_or(false) {
            let border = library
                .get(BORDER)
                .context("Missing DD default border texture")?;
            let mut points = points;
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
            let width = border.meta.size.y * scale(shape).y.abs();
            let mut outline = Item::new(
                "DD pattern outline",
                ItemKind::Path {
                    points,
                    width,
                    color: [255; 4],
                },
            )
            .with_texture(Some(BORDER));
            outline.texture_scale = border.meta.size.x * scale(shape).x.abs();
            components.push((layer, 0, outline));
        }
    }
    for path in value["pathways"].as_array().into_iter().flatten() {
        let asset = path["texture"]
            .as_str()
            .context("Prefab path has no texture")?;
        let meta = &library
            .get(asset)
            .with_context(|| format!("Missing prefab asset: {asset}"))?
            .meta;
        let original = formats::dd_points(&path["edit_points"]);
        let local = smooth_dd(
            &original,
            number(path, "smoothness", 0.),
            path["loop"].as_bool().unwrap_or(false),
        );
        let mut points: Vec<_> = local
            .into_iter()
            .map(|p| transform(p, path, "rotation"))
            .collect();
        if path["loop"].as_bool().unwrap_or(false) {
            if let Some(first) = points.first().copied() {
                points.push(first);
            }
        }
        let width = number(path, "width", 32.) / 256. * scale(path).y.abs();
        let profile = dd_profile(&points, path);
        let mut item = Item::new(
            &meta.name,
            ItemKind::Path {
                points,
                width,
                color: [255; 4],
            },
        )
        .with_texture(Some(asset));
        item.path_profile = Some(profile);
        item.texture_scale = width * meta.size.x / meta.size.y;
        item.path_style = Some(PathStyle {
            smoothness: 0.,
            transition_in: if path["grow"].as_bool().unwrap_or(false) {
                Transition::Shrink
            } else if path["fade_in"].as_bool().unwrap_or(false) {
                Transition::Fade
            } else {
                Transition::None
            },
            transition_out: if path["shrink"].as_bool().unwrap_or(false) {
                Transition::Shrink
            } else if path["fade_out"].as_bool().unwrap_or(false) {
                Transition::Fade
            } else {
                Transition::None
            },
            blocks_light: path["block_light"].as_bool().unwrap_or(false),
        });
        components.push((number(path, "layer", 100.) as i32, 1, item));
    }
    // Godot orders ZIndex first, then the Level's PatternShapes/Pathways/Objects children.
    components.sort_by_key(|(layer, category, _)| (*layer, *category));
    let mut prefab = Prefab::composition(
        definition.name.clone(),
        components.into_iter().map(|(_, _, i)| i).collect(),
    )?;
    prefab.id = format!("dd:{}", assets::hash(definition.path.as_bytes()));
    for (index, item) in prefab.items.iter_mut().enumerate() {
        item.id = format!("{}:{index}", prefab.id);
    }
    Ok(prefab)
}

pub fn import_file(path: &Path, library: &mut Library) -> Result<Prefab> {
    let file = std::fs::File::open(path)?;
    ensure!(
        file.metadata()?.len() <= assets::MAX_JSON,
        "Prefab JSON exceeds limit"
    );
    let name = path
        .file_stem()
        .context("Missing prefab name")?
        .to_string_lossy()
        .into_owned();
    let mut prefab = parse(
        &Definition {
            name,
            path: path.to_string_lossy().into_owned(),
            data: serde_json::from_reader(file)?,
        },
        library,
    )?;
    prefabs::add_preview(&mut prefab, library)?;
    Ok(prefab)
}
