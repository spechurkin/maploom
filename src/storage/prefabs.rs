//! Object compositions live in a separate library, never as flattened images.
use crate::{
    assets::{self, Asset, Library},
    document::{self, Item, ItemKind, Point},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Contents {
    pub size: Point,
    pub items: Vec<Item>,
}

/// Components are local data, never separately selectable map items.
pub fn validate_instance(item: &Item) -> Result<()> {
    let Some(contents) = &item.prefab else {
        return Ok(());
    };
    let ItemKind::Object { size, pivot, .. } = &item.kind else {
        anyhow::bail!("Prefab needs an object transform");
    };
    ensure!(
        contents.size.finite()
            && contents.size.x > 0.
            && contents.size.y > 0.
            && (size.x / contents.size.x - size.y / contents.size.y).abs() < 0.001
            && *pivot == Point::new(0.5, 0.5),
        "Invalid prefab transform"
    );
    Prefab {
        id: "instance".into(),
        name: "Prefab".into(),
        items: contents.items.clone(),
        preview: None,
    }
        .validate()
}

/// Expand only for rendering/hit testing. Stable scoped IDs isolate separate instances.
pub fn parts(item: &Item) -> Vec<Item> {
    let Some(contents) = &item.prefab else {
        return vec![item.clone()];
    };
    let ItemKind::Object {
        position,
        size,
        rotation,
        flip_x,
        flip_y,
        ..
    } = &item.kind
    else {
        return vec![];
    };
    let scale = size.x / contents.size.x;
    let sx = if *flip_x { -1. } else { 1. };
    let sy = if *flip_y { -1. } else { 1. };
    let (sin, cos) = rotation.sin_cos();
    let at = |p: Point| {
        Point::new(
            position.x + (p.x * sx * cos - p.y * sy * sin) * scale,
            position.y + (p.x * sx * sin + p.y * sy * cos) * scale,
        )
    };
    contents
        .items
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let mut child = source.clone();
            child.id = format!("{}:part:{index}", item.id);
            child.tint = crate::document::multiply_colors(child.tint, item.tint);
            child.blocks_light |= item.blocks_light;
            match &mut child.kind {
                ItemKind::Object {
                    position,
                    size,
                    rotation: angle,
                    pivot,
                    flip_x: fx,
                    flip_y: fy,
                    ..
                } => {
                    *position = at(*position);
                    size.x *= scale;
                    size.y *= scale;
                    *angle = rotation + *angle * sx * sy;
                    if *flip_x {
                        pivot.x = 1. - pivot.x;
                        *fx = !*fx;
                    }
                    if *flip_y {
                        pivot.y = 1. - pivot.y;
                        *fy = !*fy;
                    }
                }
                ItemKind::Floor { points, .. } | ItemKind::Path { points, .. } => {
                    for p in points {
                        *p = at(*p);
                    }
                    if let ItemKind::Path { width, .. } = &mut child.kind {
                        *width *= scale;
                    }
                    child.texture_scale *= scale;
                    for p in child.floor_holes.iter_mut().flatten() {
                        *p = at(*p);
                    }
                }
                _ => {}
            }
            child
        })
        .collect()
}

pub fn expanded(
    project: &crate::document::Project,
) -> std::borrow::Cow<'_, crate::document::Project> {
    if !project
        .levels
        .iter()
        .flat_map(|l| &l.layers)
        .flat_map(|l| &l.items)
        .any(|i| i.prefab.is_some())
    {
        return std::borrow::Cow::Borrowed(project);
    }
    let mut result = project.clone();
    for layer in result.levels.iter_mut().flat_map(|l| &mut l.layers) {
        layer.items = layer.items.iter().flat_map(parts).collect();
    }
    std::borrow::Cow::Owned(result)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prefab {
    pub id: String,
    pub name: String,
    /// Draw order, with positions relative to the composition's center.
    pub items: Vec<Item>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

impl Prefab {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.id.is_empty() && self.id.len() <= 512,
            "Invalid prefab ID"
        );
        ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 80,
            "Invalid prefab name"
        );
        ensure!(
            (2..=1000).contains(&self.items.len()),
            "Select 2–1000 objects for a prefab"
        );
        for item in &self.items {
            ensure!(
                item.prefab.is_none(),
                "Nested prefab components must be expanded"
            );
            if let ItemKind::Object {
                asset,
                position,
                size,
                rotation,
                pivot,
                ..
            } = &item.kind
            {
                ensure!(
                    !asset.is_empty()
                        && position.finite()
                        && size.finite()
                        && size.x > 0.
                        && size.y > 0.
                        && rotation.is_finite()
                        && pivot.finite(),
                    "Invalid prefab object"
                );
                ensure!(
                    item.texture_scale.is_finite() && item.texture_scale > 0.,
                    "Invalid prefab texture scale"
                );
                ensure!(
                    item.wall_attachment.is_none(),
                    "Prefab cannot reference map walls"
                );
            } else if let ItemKind::Path { points, width, .. } = &item.kind {
                ensure!(
                    points.len() >= 2
                        && points.iter().all(|p| p.finite())
                        && width.is_finite()
                        && *width > 0.
                        && *width <= 512.
                        && item.path_style.as_ref().is_none_or(|s| s.valid())
                        && item
                            .path_profile
                            .as_ref()
                            .is_none_or(|p| p.valid(points.len())),
                    "Invalid prefab path"
                );
            } else if let ItemKind::Floor { points, .. } = &item.kind {
                ensure!(
                    points.len() >= 3 && points.iter().all(|p| p.finite()),
                    "Invalid prefab shape"
                );
            } else {
                anyhow::bail!("Unsupported prefab component");
            }
            ensure!(
                item.texture_scale.is_finite() && item.texture_scale > 0.,
                "Invalid prefab texture scale"
            );
            ensure!(
                item.wall_attachment.is_none(),
                "Prefab cannot reference map walls"
            );
        }
        Ok(())
    }

    pub fn capture(name: String, mut items: Vec<Item>) -> Result<Self> {
        ensure!(
            (2..=1000).contains(&items.len()),
            "Select 2–1000 objects for a prefab"
        );
        ensure!(
            items
                .iter()
                .all(|i| matches!(i.kind, ItemKind::Object { .. })),
            "Prefabs contain objects only"
        );
        items = items.iter().flat_map(parts).collect();
        for item in &mut items {
            item.locked = false;
            item.wall_attachment = None;
        }
        Self::composition(name, items)
    }

    pub fn composition(name: String, mut items: Vec<Item>) -> Result<Self> {
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for item in &mut items {
            item.locked = false;
            item.wall_attachment = None;
            let points = component_points(item);
            for p in points {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
            }
        }
        let mut prefab = Self {
            id: document::id(),
            name: name.trim().into(),
            items,
            preview: None,
        };
        prefab.validate()?;
        let center = Point::new((min.x + max.x) * 0.5, (min.y + max.y) * 0.5);
        for item in &mut prefab.items {
            item.translate(Point::new(-center.x, -center.y));
        }
        prefab.validate()?;
        Ok(prefab)
    }

    pub fn place(&self, position: Point, scale: f32, rotation: f32) -> Result<Vec<Item>> {
        self.validate()?;
        ensure!(
            position.finite()
                && scale.is_finite()
                && (0.05..=100.).contains(&scale)
                && rotation.is_finite(),
            "Invalid prefab transform"
        );
        let (sin, cos) = rotation.sin_cos();
        let mut items = self.items.clone();
        for item in &mut items {
            item.id = document::id();
            item.locked = false;
            item.wall_attachment = None;
            if let ItemKind::Object {
                position: p,
                size,
                rotation: angle,
                ..
            } = &mut item.kind
            {
                *p = Point::new(
                    position.x + (p.x * cos - p.y * sin) * scale,
                    position.y + (p.x * sin + p.y * cos) * scale,
                );
                *size = Point::new(size.x * scale, size.y * scale);
                *angle += rotation;
            } else {
                let points = match &mut item.kind {
                    ItemKind::Path { points, width, .. } => {
                        *width *= scale;
                        points
                    }
                    ItemKind::Floor { points, .. } => points,
                    _ => unreachable!(),
                };
                for p in points {
                    *p = Point::new(
                        position.x + (p.x * cos - p.y * sin) * scale,
                        position.y + (p.x * sin + p.y * cos) * scale,
                    );
                }
                item.texture_scale *= scale;
            }
        }
        Ok(items)
    }

    pub fn instance(&self, position: Point, scale: f32, rotation: f32) -> Result<Item> {
        self.validate()?;
        ensure!(
            position.finite()
                && scale.is_finite()
                && (0.05..=100.).contains(&scale)
                && rotation.is_finite(),
            "Invalid prefab transform"
        );
        let points: Vec<_> = self.items.iter().flat_map(component_points).collect();
        let size = Point::new(
            points.iter().map(|p| p.x.abs()).fold(0., f32::max) * 2.,
            points.iter().map(|p| p.y.abs()).fold(0., f32::max) * 2.,
        );
        let asset = self
            .preview
            .clone()
            .or_else(|| {
                self.items
                    .iter()
                    .flat_map(Item::asset_ids)
                    .next()
                    .map(str::to_owned)
            })
            .context("Missing prefab image")?;
        let mut item = Item::new(
            &self.name,
            ItemKind::Object {
                asset,
                position,
                size: Point::new(size.x * scale, size.y * scale),
                rotation,
                pivot: Point::new(0.5, 0.5),
                flip_x: false,
                flip_y: false,
            },
        );
        item.prefab = Some(Contents {
            size,
            items: self.items.clone(),
        });
        validate_instance(&item)?;
        Ok(item)
    }
}

pub fn component_points(item: &Item) -> Vec<Point> {
    match &item.kind {
        ItemKind::Object {
            position,
            size,
            pivot,
            rotation,
            ..
        } => crate::render::object_corners(*position, *size, *pivot, *rotation).to_vec(),
        ItemKind::Path { .. } => crate::paths::item_ribbon(item)
            .into_iter()
            .map(|v| v.position)
            .collect(),
        _ => item.points(),
    }
}

/// A baked preview only; the editable component definitions remain authoritative.
pub fn add_preview(prefab: &mut Prefab, library: &mut Library) -> Result<()> {
    use crate::document::{AssetMeta, ObjectColorMode, Project};
    let points: Vec<_> = prefab.items.iter().flat_map(component_points).collect();
    let radius = points
        .iter()
        .map(|p| p.x.abs().max(p.y.abs()))
        .fold(0., f32::max);
    let side = (radius * 2. + 0.25).ceil().max(1.) as u32;
    ensure!(side <= 512, "Prefab preview exceeds map limits");
    let mut project = Project {
        width: side,
        height: side,
        background: [0; 4],
        background_asset: None,
        ..Project::default()
    };
    let offset = Point::new(side as f32 / 2., side as f32 / 2.);
    project.levels[0].layers[1].items = prefab.place(offset, 1., 0.)?;
    let (image, _) = crate::render::rasterize(
        &project,
        library,
        crate::render::ExportOptions {
            level: 0,
            pixels_per_grid: (512 / side).clamp(16, 256),
            grid: false,
            lights: false,
            jpeg_quality: 90,
        },
    )?;
    let mut bytes = std::io::Cursor::new(vec![]);
    image.write_to(&mut bytes, image::ImageFormat::Png)?;
    let bytes = bytes.into_inner();
    let digest = assets::hash(&bytes);
    let id = format!("prefab-asset:preview-{digest}");
    library.insert(
        id.clone(),
        Asset {
            meta: AssetMeta {
                id: id.clone(),
                name: format!("{} preview", prefab.name),
                path: format!("images/{digest}.png"),
                tags: vec![],
                size: Point::new(side as f32, side as f32),
                pivot: Point::new(0.5, 0.5),
                sha256: digest,
                color_mode: ObjectColorMode::None,
            },
            source: assets::Source::Bytes(std::sync::Arc::new(bytes)),
        },
    );
    prefab.preview = Some(id);
    Ok(())
}

pub fn validate(prefabs: &[Prefab], assets: &[Asset]) -> Result<()> {
    ensure!(prefabs.len() <= 1000, "Too many prefabs");
    let available: HashSet<_> = assets.iter().map(|a| a.meta.id.as_str()).collect();
    let mut ids = HashSet::new();
    for prefab in prefabs {
        prefab.validate()?;
        ensure!(ids.insert(&prefab.id), "Duplicate prefab ID");
        for asset in prefab
            .items
            .iter()
            .flat_map(Item::asset_ids)
            .chain(prefab.preview.as_deref())
        {
            ensure!(available.contains(asset), "Missing prefab asset: {asset}");
        }
    }
    Ok(())
}

/// Freeze dependencies under content-based IDs so another catalog cannot replace them.
pub fn portable(prefabs: &[Prefab], library: &Library) -> Result<(Vec<Prefab>, Vec<Asset>)> {
    let mut prefabs = prefabs.to_vec();
    let mut images = BTreeMap::new();
    let mut aliases = BTreeMap::new();
    for id in prefabs.iter().flat_map(|p| {
        p.items
            .iter()
            .flat_map(Item::asset_ids)
            .chain(p.preview.as_deref())
    }) {
        if aliases.contains_key(id) {
            continue;
        }
        let mut image = library.get(id).context("Missing prefab image")?.clone();
        let bytes = assets::verified_bytes(&image)?;
        image.meta.sha256 = assets::hash(&bytes);
        // Include metadata: tint behavior and pivots must not collide for equal pixels.
        let mut identity = image.meta.clone();
        identity.id.clear();
        identity.path.clear();
        let alias = format!(
            "prefab-asset:{}",
            assets::hash(&serde_json::to_vec(&identity)?)
        );
        aliases.insert(id.to_owned(), alias.clone());
        image.meta.id = alias.clone();
        image.source = assets::Source::Bytes(std::sync::Arc::new(bytes));
        images.insert(alias, image);
    }
    for prefab in &mut prefabs {
        if let Some(preview) = &mut prefab.preview {
            *preview = aliases[preview].clone();
        }
    }
    for item in prefabs.iter_mut().flat_map(|p| &mut p.items) {
        if let ItemKind::Object { asset, .. } = &mut item.kind {
            *asset = aliases[asset].clone();
        }
        for asset in [&mut item.texture, &mut item.border].into_iter().flatten() {
            *asset = aliases[asset].clone();
        }
    }
    let images: Vec<_> = images.into_values().collect();
    validate(&prefabs, &images)?;
    Ok((prefabs, images))
}

#[derive(Clone, Default)]
pub struct Store {
    pub personal: Vec<Prefab>,
    pub defaults: Vec<Prefab>,
    pub assets: Library,
}

impl Store {
    pub fn load(path: &Path) -> Result<Self> {
        let legacy = path.with_extension("mloom_fabric");
        let path = if !path.exists()
            && path
            .file_name()
            .is_some_and(|n| n == "prefabs.mloom_prefab")
            && legacy.is_file()
        {
            legacy.as_path()
        } else {
            path
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        let pack = assets::index_pack(path)?;
        // The personal archive is rewritten on edits. Keep live dependencies
        // independent of its entries, including copies already placed on maps.
        let mut library = Library::new();
        for mut asset in pack.assets {
            asset.source =
                assets::Source::Bytes(std::sync::Arc::new(assets::verified_bytes(&asset)?));
            library.insert(asset.meta.id.clone(), asset);
        }
        Ok(Self {
            personal: pack.prefabs,
            assets: library,
            ..Self::default()
        })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let wanted: HashSet<_> = self
            .personal
            .iter()
            .flat_map(|p| {
                p.items
                    .iter()
                    .flat_map(Item::asset_ids)
                    .chain(p.preview.as_deref())
            })
            .collect();
        let images = self
            .assets
            .values()
            .filter(|a| wanted.contains(a.meta.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        assets::save_prefab_file(path, "My prefabs", &images, &self.personal)
    }
}

pub fn path() -> std::path::PathBuf {
    crate::preferences::path().with_file_name("prefabs.mloom_prefab")
}
