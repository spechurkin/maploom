pub use super::variations::Variation;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f32 {
        (self.x - other.x).hypot(self.y - other.y)
    }
    pub fn finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
pub type Color = [u8; 4];

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextBoxShape {
    Square,
    #[default]
    Rounded,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct TextStyle {
    /// Stable local face name; an empty name uses the bundled font.
    pub font: String,
    pub background: bool,
    pub shape: TextBoxShape,
    /// Pixels at the project's native resolution, like the fixed 4 px padding.
    pub border_width: f32,
    pub background_color: Color,
    pub border_color: Color,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font: String::new(),
            background: false,
            shape: TextBoxShape::Rounded,
            border_width: 1.,
            background_color: [255; 4],
            border_color: [0, 0, 0, 255],
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortalType {
    Door,
    Window,
}

fn named_portal_type<'a>(values: impl IntoIterator<Item=&'a str>) -> Option<PortalType> {
    let mut window = false;
    for value in values {
        let value = value.to_lowercase();
        // Door wins for mixed names such as WindowDoor, including CamelCase names.
        if value.contains("door") || value.contains("двер") {
            return Some(PortalType::Door);
        }
        // Real packs also use window1, BlueLinedLongWindow and TWindowNarrow.
        window |= value.contains("window")
            || value
            .split(|c: char| !c.is_alphanumeric())
            .any(|part| matches!(part, "окно" | "окна"));
    }
    window.then_some(PortalType::Window)
}
fn asset_filename(value: &str) -> &str {
    value.rsplit(['/', '\\']).next().unwrap_or(value)
}

// Landscape stays below architecture across layers, even without water or caves.
pub fn landscape_pass(item: &Item) -> u8 {
    if item.cave.is_some() {
        2
    } else if item.water.is_some() {
        1
    } else if matches!(
        item.kind,
        ItemKind::Terrain { .. } | ItemKind::TerrainRegion { .. }
    ) {
        0
    } else {
        3
    }
}
pub fn landscape_passes(level: &Level) -> u8 {
    if level
        .layers
        .iter()
        .filter(|l| l.visible)
        .flat_map(|l| &l.items)
        .any(|i| landscape_pass(i) < 3)
    {
        4
    } else {
        1
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WaterStyle {
    pub deep_color: Color,
    pub shallow_color: Color,
    pub blend_distance: f32,
    pub disable_border: bool,
    #[serde(default = "water_smoothing")]
    pub smoothing: f32,
}
/// Cave boundaries are scenery, never architectural walls or portal hosts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CaveStyle {
    #[serde(default = "yes")]
    pub colorable: bool,
    pub ground_color: Color,
    pub wall_color: Color,
    #[serde(default)]
    pub blasts: Vec<Vec<Point>>,
}
fn water_smoothing() -> f32 {
    0.2
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectColorMode {
    #[default]
    None,
    Tint,
    RedMask,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct AssetMeta {
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub size: Point,
    pub pivot: Point,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub color_mode: ObjectColorMode,
}
impl AssetMeta {
    pub fn portal_type(&self) -> PortalType {
        self.detected_portal_type().unwrap_or(PortalType::Door)
    }

    pub fn detected_portal_type(&self) -> Option<PortalType> {
        named_portal_type([asset_filename(&self.id), &self.name])
            .or_else(|| named_portal_type(self.tags.iter().map(String::as_str)))
    }

    pub fn object_color_mode(&self) -> ObjectColorMode {
        if self.color_mode != ObjectColorMode::None {
            return self.color_mode;
        }
        if self
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case("Colorable"))
        {
            ObjectColorMode::RedMask
        } else {
            ObjectColorMode::None
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemKind {
    Roof {
        /// Open centerline; Quickbox derives it from the dragged rectangle.
        points: Vec<Point>,
        width: f32,
        roof_type: crate::roofs::RoofType,
        #[serde(default)]
        ridge: Option<String>,
        #[serde(default)]
        edge: Option<String>,
        #[serde(default)]
        hip: Option<String>,
        #[serde(default = "yes")]
        shade: bool,
        #[serde(default = "crate::roofs::default_sun")]
        sun_direction: f32,
        #[serde(default = "crate::roofs::default_contrast")]
        contrast: f32,
    },
    Room {
        rings: Vec<Vec<Point>>,
        floor: Option<String>,
        wall: Option<String>,
        width: f32,
    },
    Floor {
        points: Vec<Point>,
        color: Color,
    },
    Wall {
        points: Vec<Point>,
        width: f32,
        color: Color,
        blocks_sight: bool,
    },
    Path {
        points: Vec<Point>,
        width: f32,
        color: Color,
    },
    // Legacy brush stamps are accepted for migration only.
    Terrain {
        position: Point,
        radius: f32,
        color: Color,
    },
    TerrainRegion {
        rings: Vec<Vec<Point>>,
        color: Color,
    },
    Object {
        asset: String,
        position: Point,
        size: Point,
        rotation: f32,
        pivot: Point,
        flip_x: bool,
        flip_y: bool,
    },
    Portal {
        bounds: [Point; 2],
        closed: bool,
        color: Color,
    },
    Light {
        position: Point,
        range: f32,
        intensity: f32,
        color: Color,
        shadows: bool,
        #[serde(default = "crate::light::default_smoothing")]
        smoothing: f32,
    },
    Text {
        position: Point,
        text: String,
        size: f32,
        color: Color,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Item {
    /// Local components of one placed prefab. The Object transform owns the whole instance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefab: Option<crate::storage::prefabs::Contents>,
    pub id: String,
    pub name: String,
    pub locked: bool,
    pub kind: ItemKind,
    /// Objects optionally block light along their rotated placement bounds.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blocks_light: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_style: Option<TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_style: Option<crate::paths::PathStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path_profile: Option<crate::paths::PathProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portal_type: Option<PortalType>,
    #[serde(default)]
    pub texture: Option<String>,
    #[serde(default = "one")]
    pub texture_scale: f32,
    #[serde(default = "white")]
    pub tint: Color,
    #[serde(default = "white", skip_serializing_if = "is_white")]
    pub floor_tint: Color,
    #[serde(default = "white", skip_serializing_if = "is_white")]
    pub wall_tint: Color,
    #[serde(default)]
    pub border: Option<String>,
    #[serde(default)]
    pub wall_attachment: Option<WallAttachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water: Option<WaterStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cave: Option<CaveStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_source_rings: Option<Vec<Vec<Point>>>,
    /// Interior cutouts for a standalone floor. Rooms store cutouts in `rings`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floor_holes: Vec<Vec<Point>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_outline: Option<bool>,
    /// Room contours without walls (including the exterior contour at index 0).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wall_exclusions: Vec<usize>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WallAttachment {
    pub wall: String,
    pub segment: usize,
    pub t: f32,
    pub length: f32,
}
fn one() -> f32 {
    1.
}
fn yes() -> bool {
    true
}
fn white() -> Color {
    [255; 4]
}
fn is_white(color: &Color) -> bool {
    *color == [255; 4]
}
pub fn multiply_colors(a: Color, b: Color) -> Color {
    std::array::from_fn(|i| ((a[i] as u16 * b[i] as u16 + 127) / 255) as u8)
}
impl Item {
    pub fn portal_type(&self) -> PortalType {
        self.portal_type_with_asset(None)
    }

    pub fn portal_type_with_asset(&self, asset: Option<&AssetMeta>) -> PortalType {
        self.portal_type.unwrap_or_else(|| {
            // Older projects have only the texture and DD's light-blocking flag.
            asset
                .and_then(AssetMeta::detected_portal_type)
                .or_else(|| {
                    named_portal_type([
                        asset_filename(self.texture.as_deref().unwrap_or("")),
                        &self.name,
                    ])
                })
                .unwrap_or(
                    if matches!(self.kind, ItemKind::Portal { closed: false, .. }) {
                        PortalType::Window
                    } else {
                        PortalType::Door
                    },
                )
        })
    }

    pub fn set_room_tint(&mut self, floor: bool, color: Color) {
        // Migrate the previous shared tint without changing the other part's appearance.
        self.floor_tint = multiply_colors(self.floor_tint, self.tint);
        self.wall_tint = multiply_colors(self.wall_tint, self.tint);
        self.tint = [255; 4];
        if floor {
            self.floor_tint = color;
        } else {
            self.wall_tint = color;
        }
    }
    pub fn floor_rings(&self) -> Vec<Vec<Point>> {
        let ItemKind::Floor { points, .. } = &self.kind else {
            return vec![];
        };
        let mut rings = vec![points.clone()];
        rings.extend(self.floor_holes.clone());
        for (n, ring) in rings.iter_mut().enumerate() {
            if (crate::geometry::area(ring) > 0.) != (n == 0) {
                ring.reverse();
            }
        }
        rings
    }
    pub fn wall_segments(&self) -> Vec<[Point; 2]> {
        match &self.kind {
            ItemKind::Wall { points, .. } => points.windows(2).map(|w| [w[0], w[1]]).collect(),
            ItemKind::Room {
                rings,
                wall: Some(_),
                ..
            } => rings
                .iter()
                .enumerate()
                .filter(|(n, _)| !self.wall_exclusions.contains(n))
                .map(|(_, r)| r)
                .flat_map(|r| (0..r.len()).map(|i| [r[i], r[(i + 1) % r.len()]]))
                .collect(),
            _ => vec![],
        }
    }
    pub fn new(name: impl Into<String>, kind: ItemKind) -> Self {
        Self {
            prefab: None,
            id: id(),
            name: name.into(),
            locked: false,
            kind,
            blocks_light: false,
            text_style: None,
            path_style: None,
            path_profile: None,
            portal_type: None,
            texture: None,
            texture_scale: 1.,
            tint: [255; 4],
            floor_tint: [255; 4],
            wall_tint: [255; 4],
            border: None,
            wall_attachment: None,
            water: None,
            cave: None,
            water_source_rings: None,
            floor_holes: vec![],
            floor_outline: None,
            wall_exclusions: vec![],
        }
    }
    pub fn with_texture(mut self, texture: Option<&str>) -> Self {
        self.texture = texture.map(str::to_owned);
        self
    }
    pub fn asset_ids(&self) -> Vec<&str> {
        let mut ids = vec![];
        if let Some(prefab) = &self.prefab {
            ids.extend(prefab.items.iter().flat_map(Item::asset_ids));
        }
        if let Some(texture) = &self.texture {
            ids.push(texture.as_str());
        }
        if let Some(border) = &self.border {
            ids.push(border.as_str());
        }
        if let ItemKind::Roof {
            ridge, edge, hip, ..
        } = &self.kind
        {
            ids.extend(
                [ridge, edge, hip]
                    .into_iter()
                    .filter_map(|id| id.as_deref()),
            );
        }
        if let ItemKind::Object { asset, .. } = &self.kind {
            ids.push(asset);
        }
        if let ItemKind::Room { floor, wall, .. } = &self.kind {
            if let Some(id) = floor {
                ids.push(id);
            }
            if let Some(id) = wall {
                ids.push(id);
            }
        }
        ids
    }
    pub fn points(&self) -> Vec<Point> {
        match &self.kind {
            ItemKind::Room { rings, .. } | ItemKind::TerrainRegion { rings, .. } => {
                rings.iter().flatten().copied().collect()
            }
            ItemKind::Floor { points, .. }
            | ItemKind::Roof { points, .. }
            | ItemKind::Wall { points, .. }
            | ItemKind::Path { points, .. } => points.clone(),
            ItemKind::Portal { bounds, .. } => bounds.to_vec(),
            ItemKind::Object { position, .. }
            | ItemKind::Terrain { position, .. }
            | ItemKind::Light { position, .. }
            | ItemKind::Text { position, .. } => vec![*position],
        }
    }
    pub fn translate(&mut self, delta: Point) {
        let shift = |p: &mut Point| {
            p.x += delta.x;
            p.y += delta.y;
        };
        if let Some(rings) = &mut self.water_source_rings {
            rings.iter_mut().flatten().for_each(shift);
        }
        if let Some(cave) = &mut self.cave {
            cave.blasts.iter_mut().flatten().for_each(shift);
        }
        self.floor_holes.iter_mut().flatten().for_each(shift);
        match &mut self.kind {
            ItemKind::Room { rings, .. } | ItemKind::TerrainRegion { rings, .. } => {
                rings.iter_mut().flatten().for_each(shift)
            }
            ItemKind::Floor { points, .. }
            | ItemKind::Roof { points, .. }
            | ItemKind::Wall { points, .. }
            | ItemKind::Path { points, .. } => points.iter_mut().for_each(shift),
            ItemKind::Portal { bounds, .. } => bounds.iter_mut().for_each(shift),
            ItemKind::Object { position, .. }
            | ItemKind::Terrain { position, .. }
            | ItemKind::Light { position, .. }
            | ItemKind::Text { position, .. } => shift(position),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub items: Vec<Item>,
}
impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: id(),
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            items: vec![],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Level {
    pub id: String,
    pub name: String,
    pub layers: Vec<Layer>,
}
impl Level {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: id(),
            name: name.into(),
            layers: vec![
                Layer::new("Ground"),
                Layer::new("Architecture"),
                Layer::new("Objects"),
                Layer::new("Lighting"),
            ],
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PackRef {
    pub name: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CatalogRef {
    pub id: String,
    pub name: String,
    /// Relative pack paths; an empty path selects loose images. None is legacy all-packs selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub packs: Option<Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Project {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub pixels_per_grid: u32,
    pub background: Color,
    #[serde(default = "white")]
    pub ambient_light: Color,
    #[serde(default)]
    pub background_asset: Option<String>,
    #[serde(default = "yes")]
    pub use_default_assets: bool,
    pub levels: Vec<Level>,
    /// Stable reference to the main floor. Legacy documents use the first floor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_level: Option<String>,
    /// Root scene fields are the active working view for legacy renderers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_variation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variations: Vec<Variation>,
    #[serde(default)]
    pub assets: BTreeMap<String, AssetMeta>,
    #[serde(default)]
    pub packs: Vec<PackRef>,
    #[serde(default)]
    pub asset_catalogs: Vec<CatalogRef>,
    /// Asset ID -> catalog ID. Local filesystem paths belong to preferences only.
    #[serde(default)]
    pub external_assets: BTreeMap<String, String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Default for Project {
    fn default() -> Self {
        Self {
            format: "maploom/project".into(),
            version: 1,
            id: id(),
            name: "Untitled map".into(),
            width: 32,
            height: 24,
            pixels_per_grid: 100,
            background: [40, 55, 51, 255],
            ambient_light: white(),
            background_asset: Some("res://textures/terrain/terrain_grass.png".into()),
            use_default_assets: true,
            levels: vec![Level::new("Ground floor")],
            main_level: None,
            active_variation: None,
            variations: vec![],
            assets: BTreeMap::new(),
            packs: vec![],
            asset_catalogs: vec![],
            external_assets: BTreeMap::new(),
            extensions: BTreeMap::new(),
        }
    }
}
impl Project {
    pub fn main_level_index(&self) -> usize {
        self.main_level
            .as_ref()
            .and_then(|id| self.levels.iter().position(|l| &l.id == id))
            .unwrap_or(0)
    }

    pub fn protect_main_floor(&self, next: &mut Self) -> Result<()> {
        let main = &self.levels[self.main_level_index()].id;
        ensure!(
            next.main_level.as_ref().is_none_or(|id| id == main),
            "Main floor is fixed"
        );
        let next_main = next
            .levels
            .iter()
            .position(|l| &l.id == main)
            .ok_or_else(|| anyhow::anyhow!("Main floor is fixed"))?;
        for (index, floor) in self.levels.iter().enumerate() {
            if let Some(target) = next.levels.iter().position(|l| l.id == floor.id) {
                ensure!(
                    index.cmp(&self.main_level_index()) == target.cmp(&next_main),
                    "Main floor is fixed"
                );
            }
        }
        if self.main_level.is_some() || next_main != 0 {
            next.main_level = Some(main.clone());
        }
        Ok(())
    }

    /// Isolated floor content, with an opaque basement or transparent upper floor.
    pub fn floor_surface(&self, level: usize) -> Self {
        let mut surface = self.scene_copy();
        if level != self.main_level_index() {
            surface.background = if level < self.main_level_index() {
                [0, 0, 0, 255]
            } else {
                [0; 4]
            };
            surface.background_asset = None;
        }
        surface.levels = vec![self.levels[level].clone()];
        surface.main_level = None;
        surface
    }

    /// Export preserves registration on the full map, without editor-only backgrounds.
    pub fn export_surface(&self, level: usize) -> Self {
        let mut surface = self.floor_surface(level);
        if level != self.main_level_index() {
            surface.background = [0; 4];
            surface.background_asset = None;
        }
        surface
    }

    /// Older maps stored each brush sample as an item. Collapse consecutive
    /// samples with the same appearance without crossing the paint order.
    pub fn migrate_terrain(&mut self) {
        for layer in self.all_levels_mut().flat_map(|level| &mut level.layers) {
            let mut items = std::mem::take(&mut layer.items).into_iter().peekable();
            while let Some(mut item) = items.next() {
                if let ItemKind::Terrain {
                    position,
                    radius,
                    color,
                } = item.kind
                {
                    let mut stamps = vec![(position, radius)];
                    while items.peek().is_some_and(|next| {
                        matches!(next.kind, ItemKind::Terrain { color: c, .. } if c == color)
                            && next.texture == item.texture
                            && next.border == item.border
                            && next.tint == item.tint
                            && next.texture_scale == item.texture_scale
                            && next.locked == item.locked
                            && next.name == item.name
                    }) {
                        if let ItemKind::Terrain {
                            position, radius, ..
                        } = items.next().unwrap().kind
                        {
                            stamps.push((position, radius));
                        }
                    }
                    item.kind = ItemKind::TerrainRegion {
                        rings: crate::geometry::terrain_contours(&stamps),
                        color,
                    };
                }
                layer.items.push(item);
            }
        }
    }
    pub fn sync_portals(&mut self) {
        for level in self.all_levels_mut() {
            let walls: BTreeMap<_, _> = level
                .layers
                .iter()
                .flat_map(|l| &l.items)
                .filter_map(|i| {
                    let lines = i.wall_segments();
                    (!lines.is_empty()).then_some((i.id.clone(), i.clone()))
                })
                .collect();
            for item in level.layers.iter_mut().flat_map(|l| &mut l.items) {
                if let (Some(attachment), ItemKind::Portal { bounds, .. }) =
                    (&item.wall_attachment, &mut item.kind)
                {
                    if let Some(geometry) = walls
                        .get(&attachment.wall)
                        .and_then(|wall| crate::portals::resolve(wall, attachment))
                    {
                        *bounds = geometry.bounds;
                    } else {
                        item.wall_attachment = None;
                    }
                }
            }
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.validate_variations()?;
        ensure!(
            self.format == "maploom/project" && self.version == 1,
            "Unsupported Maploom project version"
        );
        ensure!(
            (1..=512).contains(&self.width) && (1..=512).contains(&self.height),
            "Map dimensions must be 1–512 grid cells"
        );
        ensure!(
            (16..=512).contains(&self.pixels_per_grid),
            "Pixels per grid must be 16–512"
        );
        ensure!(
            !self.levels.is_empty() && self.levels.len() <= 256,
            "Project needs 1–256 levels"
        );
        let mut ids = HashSet::new();
        ensure!(
            self.main_level
                .as_ref()
                .is_none_or(|id| self.levels.iter().any(|l| &l.id == id)),
            "Invalid main floor"
        );
        for level in &self.levels {
            ensure!(
                ids.insert(&level.id) && !level.layers.is_empty(),
                "Duplicate ID or empty level"
            );
            for layer in &level.layers {
                ensure!(
                    ids.insert(&layer.id)
                        && layer.opacity.is_finite()
                        && (0.0..=1.0).contains(&layer.opacity),
                    "Invalid layer"
                );
                for item in &layer.items {
                    crate::storage::prefabs::validate_instance(item)?;
                    if let Some(cave) = &item.cave {
                        ensure!(
                            matches!(item.kind, ItemKind::TerrainRegion { .. })
                                && item.water.is_none(),
                            "Invalid cave material"
                        );
                        ensure!(
                            cave.blasts
                                .iter()
                                .all(|r| r.len() >= 3 && r.iter().all(|p| p.finite())),
                            "Invalid cave blast geometry"
                        );
                    }
                    if let Some(water) = &item.water {
                        ensure!(
                            matches!(item.kind, ItemKind::TerrainRegion { .. })
                                && water.blend_distance.is_finite()
                                && (0.0..=32.).contains(&water.blend_distance)
                                && water.smoothing.is_finite()
                                && (0.0..=1.).contains(&water.smoothing),
                            "Invalid water material"
                        );
                    }
                    if let Some(rings) = &item.water_source_rings {
                        ensure!(
                            item.water.is_some()
                                && !rings.is_empty()
                                && rings
                                    .iter()
                                    .all(|r| r.len() >= 3 && r.iter().all(|p| p.finite())),
                            "Invalid source water geometry"
                        );
                    }
                    ensure!(
                        item.texture_scale.is_finite() && item.texture_scale > 0.,
                        "Invalid texture scale"
                    );
                    ensure!(
                        ids.insert(&item.id) && item.points().iter().all(|p| p.finite()),
                        "Duplicate item ID or invalid coordinates"
                    );
                    ensure!(
                        item.floor_holes.is_empty() || matches!(item.kind, ItemKind::Floor { .. }),
                        "Cutouts require a floor"
                    );
                    ensure!(
                        item.floor_holes
                            .iter()
                            .all(|r| r.len() >= 3 && r.iter().all(|p| p.finite())),
                        "Invalid floor cutout"
                    );
                    ensure!(
                        item.wall_exclusions.is_empty()
                            || matches!(&item.kind, ItemKind::Room { rings, .. } if item.wall_exclusions.iter().all(|n| *n < rings.len())),
                        "Invalid wall contour"
                    );
                    ensure!(item.path_style.as_ref().is_none_or(|s| matches!(item.kind, ItemKind::Path { .. }) && s.valid()), "Invalid path style");
                    ensure!(item.path_profile.as_ref().is_none_or(|p| matches!(&item.kind, ItemKind::Path { points, .. } if p.valid(points.len()))), "Invalid sampled path profile");
                    match &item.kind {
                        ItemKind::Roof {
                            points,
                            width,
                            roof_type,
                            sun_direction,
                            contrast,
                            ..
                        } => {
                            ensure!(
                                crate::roofs::valid(points, *width, *roof_type),
                                "Invalid roof geometry"
                            );
                            ensure!(
                                sun_direction.is_finite()
                                    && contrast.is_finite()
                                    && (0.0..=1.).contains(contrast),
                                "Invalid roof shading"
                            );
                        }
                        ItemKind::Room { rings, width, .. } => ensure!(
                            !rings.is_empty()
                                && rings.iter().all(|r| r.len() >= 3)
                                && width.is_finite()
                                && *width > 0.,
                            "Invalid room geometry"
                        ),
                        ItemKind::Floor { points, .. } => {
                            ensure!(points.len() >= 3, "Floor needs three points");
                            ensure!(
                                item.floor_holes.is_empty()
                                    || crate::geometry::valid_floor_cutouts(
                                        points,
                                        &item.floor_holes
                                    ),
                                "Invalid floor cutout topology"
                            );
                        }
                        ItemKind::Path { points, width, .. } => ensure!(
                            points.len() >= 2 && width.is_finite() && *width > 0. && *width <= 512.,
                            "Path width must be positive and at most 512"
                        ),
                        ItemKind::Wall { points, width, .. } => ensure!(
                            points.len() >= 2 && width.is_finite() && *width > 0.0,
                            "Invalid line"
                        ),
                        ItemKind::TerrainRegion { rings, .. } => ensure!(
                            !rings.is_empty() && rings.iter().all(|r| r.len() >= 3),
                            "Invalid terrain geometry"
                        ),
                        ItemKind::Terrain { radius, .. } => {
                            ensure!(radius.is_finite() && *radius > 0.0, "Invalid brush radius")
                        }
                        ItemKind::Object {
                            size,
                            rotation,
                            pivot,
                            ..
                        } => ensure!(
                            size.finite()
                                && size.x > 0.0
                                && size.y > 0.0
                                && rotation.is_finite()
                                && pivot.finite(),
                            "Invalid object transform"
                        ),
                        ItemKind::Light {
                            range,
                            intensity,
                            smoothing,
                            ..
                        } => ensure!(
                            range.is_finite()
                                && *range > 0.0
                                && intensity.is_finite()
                                && *intensity >= 0.0
                                && smoothing.is_finite()
                                && (0.0..=1.0).contains(smoothing),
                            "Invalid light"
                        ),
                        ItemKind::Text { size, .. } => {
                            ensure!(size.is_finite() && *size > 0.0, "Invalid text size");
                            if let Some(style) = &item.text_style {
                                ensure!(
                                    style.border_width.is_finite()
                                        && (0.0..=64.0).contains(&style.border_width),
                                    "Invalid text border"
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        for (key, asset) in &self.assets {
            ensure!(
                key == &asset.id
                    && asset.size.finite()
                    && asset.size.x > 0.0
                    && asset.size.y > 0.0
                    && asset.pivot.finite(),
                "Invalid asset metadata"
            );
        }
        Ok(())
    }
    pub fn demo() -> Self {
        let mut p = Self {
            name: "The Mosslight Sanctuary".into(),
            ..Self::default()
        };
        let regions: Vec<_> = [
            (5., 4., 10., 8.),
            (18., 4., 8., 8.),
            (12., 15., 12., 5.),
            (14., 7., 6., 3.),
            (20., 10., 3., 7.),
        ]
            .into_iter()
            .map(|(x, y, w, h)| {
                vec![vec![
                    Point::new(x, y),
                    Point::new(x + w, y),
                    Point::new(x + w, y + h),
                    Point::new(x, y + h),
                ]]
            })
            .collect();
        let mut room = Item::new(
            "Sanctuary rooms",
            ItemKind::Room {
                rings: crate::geometry::union_rooms(&regions).expect("Demo room union"),
                floor: Some("res://textures/tilesets/simple/tileset_cut_stone.png".into()),
                wall: Some("res://textures/walls/stone.png".into()),
                width: 0.24,
            },
        );
        room.texture_scale = 4.;
        room.floor_tint = [199, 181, 145, 255];
        room.wall_tint = [122, 137, 125, 255];
        let room_id = room.id.clone();
        let lines = room.wall_segments();
        let segment = lines
            .iter()
            .position(|line| line[0].x == 5. && line[1].x == 5.)
            .unwrap_or(0);
        p.levels[0].layers[1].items.push(room);
        let mut door = Item::new(
            "door_06",
            ItemKind::Portal {
                bounds: [Point::new(5., 7.), Point::new(5., 8.)],
                closed: true,
                color: [255; 4],
            },
        )
            .with_texture(Some("res://textures/portals/door_06.png"));
        door.wall_attachment = Some(WallAttachment {
            wall: room_id.clone(),
            segment,
            t: 0.5,
            length: 1.,
        });
        p.levels[0].layers[1].items.push(door);
        // Attach windows to the union's exterior, so they cut the wall and follow room edits.
        for (x, y, variant) in [
            (8., 4., "window_12"),
            (12., 4., "window_12"),
            (22., 4., "window_04"),
            (26., 7., "window_12"),
            (26., 10., "window_12"),
            (5., 10., "window_04"),
            (18., 20., "window_12"),
            (22., 20., "window_12"),
            (12., 17., "window_04"),
        ] {
            let center = Point::new(x, y);
            let segment = lines
                .iter()
                .position(|[a, b]| {
                    (a.distance(center) + center.distance(*b) - a.distance(*b)).abs() < 0.001
                })
                .expect("Demo window on exterior wall");
            let [a, b] = lines[segment];
            let texture = format!("res://textures/portals/{variant}.png");
            let name = if variant == "window_04" {
                "Open shutter window"
            } else {
                "Glass window"
            };
            let mut window = Item::new(
                name,
                ItemKind::Portal {
                    bounds: [center; 2],
                    closed: false,
                    color: [255; 4],
                },
            )
                .with_texture(Some(&texture));
            window.wall_attachment = Some(WallAttachment {
                wall: room_id.clone(),
                segment,
                t: a.distance(center) / a.distance(b),
                length: 1.4,
            });
            p.levels[0].layers[1].items.push(window);
        }
        p.sync_portals();
        let mut approach = Item::new(
            "Woodland approach",
            ItemKind::Path {
                points: [
                    (8., 24.),
                    (7., 23.3),
                    (5.8, 22.5),
                    (4.6, 21.6),
                    (3.7, 20.5),
                    (3., 19.2),
                    (2.6, 17.8),
                    (2.4, 16.2),
                    (2.4, 14.5),
                    (2.5, 12.8),
                    (2.7, 11.2),
                    (3., 9.8),
                    (3.4, 8.8),
                    (4.1, 8.2),
                    (5., 8.),
                ]
                    .into_iter()
                    .map(|(x, y)| Point::new(x, y))
                    .collect(),
                width: 1.35,
                color: [255; 4],
            },
        )
            .with_texture(Some("res://textures/paths/path_blender_03.png"));
        approach.texture_scale = 4.;
        p.levels[0].layers[0].items.push(approach);
        // Rugs go below the furniture; the central aisle and connecting passages stay clear.
        let objects = &mut p.levels[0].layers[2].items;
        let mut furnish = |name: &str, asset: &str, x, y, w, h, degrees: f32, tint| {
            let mut item = Item::new(
                name,
                ItemKind::Object {
                    asset: format!("res://textures/objects/{asset}.png"),
                    position: Point::new(x, y),
                    size: Point::new(w, h),
                    rotation: degrees.to_radians(),
                    pivot: Point::new(0.5, 0.5),
                    flip_x: false,
                    flip_y: false,
                },
            );
            item.tint = tint;
            objects.push(item);
        };
        let ivory = [255; 4];
        let moss = [72, 112, 101, 255];
        let wine = [118, 57, 49, 255];
        furnish(
            "Processional runner",
            "decor/floor/carpet_02",
            10.,
            8.8,
            2.1,
            3.6,
            0.,
            moss,
        );
        furnish(
            "Reading room rug",
            "decor/floor/carpet_04",
            22.,
            8.,
            4.3,
            4.3,
            0.,
            moss,
        );
        furnish(
            "Stone altar",
            "magic/altar_01",
            10.,
            5.3,
            2.,
            1.46,
            0.,
            wine,
        );
        furnish(
            "Sanctuary statue",
            "decor/statues/statue_01",
            6.5,
            5.4,
            1.1,
            1.1,
            0.,
            ivory,
        );
        furnish(
            "Sanctuary statue",
            "decor/statues/statue_01",
            13.5,
            5.4,
            1.1,
            1.1,
            0.,
            ivory,
        );
        for x in [7.4, 12.6] {
            for y in [8.1, 10.] {
                furnish(
                    "Prayer bench",
                    "furniture/chairs/bench_01",
                    x,
                    y,
                    2.1,
                    0.72,
                    0.,
                    ivory,
                );
            }
        }
        furnish(
            "Altar candles",
            "activities/administration/candle_holder_02",
            10.,
            5.1,
            0.75,
            0.62,
            0.,
            ivory,
        );
        furnish(
            "Maps and manuscripts",
            "activities/administration/map_table_big_01",
            22.,
            8.,
            2.5,
            2.22,
            0.,
            ivory,
        );
        furnish(
            "Reader's chair",
            "furniture/chairs/chair_01",
            22.,
            6.3,
            0.85,
            0.81,
            180.,
            ivory,
        );
        furnish(
            "Reader's chair",
            "furniture/chairs/chair_01",
            22.,
            9.7,
            0.85,
            0.81,
            0.,
            ivory,
        );
        furnish(
            "Reader's chair",
            "furniture/chairs/chair_01",
            20.1,
            8.,
            0.85,
            0.81,
            90.,
            ivory,
        );
        furnish(
            "Reader's chair",
            "furniture/chairs/chair_01",
            23.9,
            8.,
            0.85,
            0.81,
            -90.,
            ivory,
        );
        furnish(
            "Archive shelves",
            "furniture/storage/merchant_shelf_01",
            19.5,
            4.6,
            2.,
            0.96,
            0.,
            ivory,
        );
        furnish(
            "Archive shelves",
            "furniture/storage/merchant_shelf_03",
            25.4,
            5.7,
            2.,
            0.96,
            90.,
            ivory,
        );
        furnish(
            "Reading candle",
            "activities/administration/candle_01",
            22.65,
            8.2,
            0.28,
            0.29,
            0.,
            ivory,
        );
        furnish(
            "Supplies shelf",
            "furniture/storage/merchant_shelf_02",
            15.,
            15.6,
            2.4,
            1.15,
            0.,
            ivory,
        );
        furnish(
            "Provision crate",
            "supplies/crates/crate_01",
            13.,
            18.9,
            1.05,
            0.99,
            0.,
            ivory,
        );
        furnish(
            "Provision crate",
            "supplies/crates/crate_01",
            14.2,
            18.9,
            0.9,
            0.85,
            8.,
            ivory,
        );
        furnish(
            "Water barrel",
            "supplies/barrels/barrel_01",
            15.8,
            19.,
            0.95,
            0.91,
            0.,
            ivory,
        );
        furnish(
            "Water barrel",
            "supplies/barrels/barrel_01",
            17.,
            19.,
            0.95,
            0.91,
            12.,
            ivory,
        );
        furnish(
            "Keeper's table",
            "furniture/tables/small_table_01",
            20.5,
            17.8,
            2.6,
            1.52,
            0.,
            ivory,
        );
        furnish(
            "Keeper's bench",
            "furniture/chairs/bench_01",
            20.5,
            19.1,
            2.1,
            0.72,
            180.,
            ivory,
        );
        furnish(
            "Keeper's journal",
            "activities/administration/book_06",
            20.2,
            17.8,
            0.65,
            0.45,
            -12.,
            ivory,
        );
        furnish(
            "Table candle",
            "activities/administration/candle_02",
            21.2,
            17.8,
            0.3,
            0.3,
            0.,
            ivory,
        );
        furnish(
            "Offering chest",
            "furniture/storage/chest_01",
            23.,
            18.8,
            1.2,
            0.88,
            90.,
            ivory,
        );
        for (x, y) in [(7., 6.), (24., 6.), (14., 17.)] {
            furnish(
                "Brass lantern",
                "decor/lighting/lantern_01",
                x,
                y,
                0.45,
                0.43,
                0.,
                ivory,
            );
        }
        for (x, y) in [(7., 6.), (24., 6.), (14., 17.)] {
            p.levels[0].layers[2].items.push(Item::new(
                "Warm lantern",
                ItemKind::Light {
                    position: Point::new(x, y),
                    range: 2.5,
                    intensity: 0.65,
                    smoothing: 1.,
                    color: [255, 199, 107, 255],
                    shadows: true,
                },
            ));
        }
        p.levels[0].layers[2].items.push(Item::new(
            "Sanctuary",
            ItemKind::Text {
                position: Point::new(10., 11.3),
                text: "SANCTUARY".into(),
                size: 0.28,
                color: [47, 54, 50, 255],
            },
        ));
        // Trees stand directly on the grass, leaving the approach and the walls clear.
        for (variant, x, y, width, height, degrees) in [
            ("aspen_01", 3., 4., 2.5, 2.23, 0_f32),
            ("aspen_01", 28., 7., 2.5, 2.23, 35.),
            ("aspen_01", 6., 18., 2.5, 2.23, -20.),
            ("aspen_01", 27., 20., 2.5, 2.23, 80.),
            ("birch_01", 2., 1.5, 2.3, 2.25, 25.),
            ("birch_01", 7., 1.7, 2.4, 2.34, -15.),
            ("aspen_01", 11.5, 1.8, 2.7, 2.4, 70.),
            ("birch_01", 16., 2., 2.6, 2.54, 40.),
            ("aspen_01", 20.5, 1.5, 2.5, 2.23, -30.),
            ("birch_01", 25.2, 1.8, 2.7, 2.64, 90.),
            ("aspen_01", 29.8, 2.2, 2.4, 2.14, 15.),
            ("oak_01", 29.4, 11.4, 3.4, 2.96, 15.),
            ("birch_01", 28.2, 15.7, 3.1, 3.03, -35.),
            ("aspen_01", 30.3, 20.3, 2.6, 2.32, 45.),
            ("oak_01", 25.8, 22.4, 2.8, 2.44, -20.),
            ("birch_01", 21., 22.4, 2.6, 2.54, 20.),
            ("aspen_01", 16.7, 22.3, 3.1, 2.76, 55.),
            ("oak_01", 11.8, 22.4, 2.7, 2.35, 10.),
            ("birch_01", 8.4, 21.1, 2.4, 2.34, -50.),
            ("birch_01", 8.9, 15.2, 2.5, 2.44, 35.),
            ("aspen_01", 6.2, 14.4, 2.1, 1.87, 10.),
            ("birch_01", 1.1, 7., 1.8, 1.76, -10.),
            ("aspen_01", 0.9, 11.5, 1.7, 1.51, 75.),
            ("birch_01", 1.1, 17.5, 1.7, 1.66, 15.),
        ] {
            p.levels[0].layers[2].items.push(Item::new(
                "Tree",
                ItemKind::Object {
                    asset: format!("res://textures/objects/more_trees/{variant}.png"),
                    position: Point::new(x, y),
                    size: Point::new(width, height),
                    rotation: degrees.to_radians(),
                    pivot: Point::new(0.5, 0.5),
                    flip_x: false,
                    flip_y: false,
                },
            ));
        }
        p.migrate_terrain();
        p
    }
}
#[derive(Default)]
pub struct History {
    past: Vec<Project>,
    future: Vec<Project>,
}
impl History {
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn record(&mut self, before: Project) {
        self.past.push(before);
        if self.past.len() > 80 {
            self.past.remove(0);
        }
        self.future.clear();
    }
    pub fn undo(&mut self, project: &mut Project) -> bool {
        if let Some(p) = self.past.pop() {
            self.future.push(std::mem::replace(project, p));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, project: &mut Project) -> bool {
        if let Some(p) = self.future.pop() {
            self.past.push(std::mem::replace(project, p));
            true
        } else {
            false
        }
    }
}
