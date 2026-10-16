use crate::document::{AssetMeta, Point, id};
use anyhow::{Context, Result, bail, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

pub const MAX_ENTRY: u64 = 256 * 1024 * 1024;
pub const MAX_JSON: u64 = 64 * 1024 * 1024;
pub const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;

/// Exposed alpha pixel edges, in texture coordinates. Merge straight runs while
/// retaining concavities, holes and disconnected parts of the artwork.
pub fn alpha_boundary(image: &image::RgbaImage) -> Vec<[Point; 2]> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return vec![];
    }
    let mut segments = vec![];
    let opaque = |x, y| image.get_pixel(x, y)[3] > 0;
    for vertical in [false, true] {
        let (along, across) = if vertical {
            (height, width)
        } else {
            (width, height)
        };
        for edge in 0..=across {
            let mut start = None;
            for offset in 0..=along {
                let exposed = offset < along && {
                    let at = |side| {
                        if vertical {
                            opaque(side, offset)
                        } else {
                            opaque(offset, side)
                        }
                    };
                    (edge > 0 && at(edge - 1)) != (edge < across && at(edge))
                };
                if exposed {
                    start.get_or_insert(offset);
                } else if let Some(first) = start.take() {
                    let at = |n| {
                        let (x, y) = if vertical { (edge, n) } else { (n, edge) };
                        Point::new(x as f32 / width as f32, y as f32 / height as f32)
                    };
                    segments.push([at(first), at(offset)]);
                }
            }
        }
    }
    segments
}

/// Convex silhouette of nontransparent pixel cells in normalized texture coordinates.
/// Row extrema preserve every support direction without shipping an alpha bitmap.
pub fn alpha_silhouette(image: &image::RgbaImage) -> Vec<Point> {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return vec![];
    }
    let mut points: Vec<(i64, i64)> = vec![];
    for y in 0..height {
        let mut first = None;
        let mut last = 0;
        for x in 0..width {
            if image.get_pixel(x, y).0[3] > 0 {
                first.get_or_insert(x);
                last = x;
            }
        }
        if let Some(x) = first {
            points.extend([
                (x as i64, y as i64),
                ((last + 1) as i64, y as i64),
                (x as i64, (y + 1) as i64),
                ((last + 1) as i64, (y + 1) as i64),
            ]);
        }
    }
    points.sort_unstable();
    points.dedup();
    let cross = |a: (i64, i64), b: (i64, i64), c: (i64, i64)| {
        (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
    };
    let mut hull = vec![];
    for &p in &points {
        while hull.len() >= 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    let lower = hull.len();
    for &p in points.iter().rev().skip(1) {
        while hull.len() > lower && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0 {
            hull.pop();
        }
        hull.push(p);
    }
    hull.pop();
    hull.into_iter()
        .map(|(x, y)| Point::new(x as f32 / width as f32, y as f32 / height as f32))
        .collect()
}
#[derive(Clone, Debug)]
pub enum Source {
    Catalog {
        id: String,
        source: Box<Source>,
    },
    File(PathBuf),
    Zip {
        path: PathBuf,
        entry: String,
    },
    Pck {
        path: PathBuf,
        offset: u64,
        size: u64,
    },
    Bytes(Arc<Vec<u8>>),
    Godot(Box<Source>),
}
impl Source {
    pub fn read(&self) -> Result<Vec<u8>> {
        match self {
            Self::Catalog { source, .. } => source.read(),
            Self::Godot(source) => stex_png(&source.read()?),
            Self::Bytes(bytes) => Ok(bytes.as_ref().clone()),
            Self::File(path) => {
                let f = File::open(path)?;
                ensure!(f.metadata()?.len() <= MAX_ENTRY, "Image exceeds 256 MiB");
                let mut bytes = vec![];
                f.take(MAX_ENTRY + 1).read_to_end(&mut bytes)?;
                ensure!(bytes.len() as u64 <= MAX_ENTRY, "Image exceeds limit");
                Ok(bytes)
            }
            Self::Zip { path, entry } => {
                let mut zip = ZipArchive::new(File::open(path)?)?;
                read_zip(&mut zip, entry, MAX_ENTRY)
            }
            Self::Pck { path, offset, size } => {
                ensure!(*size <= MAX_ENTRY, "PCK image too large");
                let mut f = File::open(path)?;
                ensure!(
                    offset
                        .checked_add(*size)
                        .is_some_and(|end| end <= f.metadata().map(|m| m.len()).unwrap_or(0)),
                    "PCK source changed or truncated"
                );
                f.seek(SeekFrom::Start(*offset))?;
                let mut bytes = vec![0; *size as usize];
                f.read_exact(&mut bytes)?;
                Ok(bytes)
            }
        }
    }
}
pub fn stex_png(bytes: &[u8]) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() >= 20 && &bytes[..4] == b"GDST",
        "Not a Godot 3 stream texture"
    );
    let w = u16::from_le_bytes(bytes[4..6].try_into()?) as u32;
    let h = u16::from_le_bytes(bytes[8..10].try_into()?) as u32;
    ensure!(
        w > 0 && h > 0 && (w as u64 * h as u64) <= 16 * 1024 * 1024,
        "Godot texture exceeds decode budget"
    );
    let df = u32::from_le_bytes(bytes[16..20].try_into()?);
    let img = if df & ((1 << 20) | (1 << 21)) != 0 {
        ensure!(bytes.len() >= 28, "Truncated Godot texture");
        let len = u32::from_le_bytes(bytes[24..28].try_into()?) as usize;
        let data = bytes
            .get(28..28 + len)
            .context("Truncated Godot image payload")?;
        let data = if image::guess_format(data).is_ok() {
            data
        } else {
            data.get(4..).context("Invalid Godot packed image")?
        };
        decode(data)?
    } else {
        let fmt = df & ((1 << 20) - 1);
        let channels = match fmt {
            0 => 1,
            1 => 2,
            2 => 3,
            3 => 4,
            _ => anyhow::bail!("Unsupported Godot image format {fmt}"),
        };
        let count = w as usize * h as usize;
        let data = bytes
            .get(20..20 + count * channels)
            .context("Truncated raw Godot texture")?;
        let mut out = Vec::with_capacity(count * 4);
        for p in data.chunks_exact(channels) {
            let rgba = match channels {
                1 => [p[0], p[0], p[0], 255],
                2 => [p[0], p[0], p[0], p[1]],
                3 => [p[0], p[1], p[2], 255],
                _ => [p[0], p[1], p[2], p[3]],
            };
            out.extend_from_slice(&rgba);
        }
        image::RgbaImage::from_raw(w, h, out).context("Invalid raw Godot texture")?
    };
    let mut png = std::io::Cursor::new(vec![]);
    img.write_to(&mut png, image::ImageFormat::Png)?;
    Ok(png.into_inner())
}
pub fn dd_defaults(pck: &Path) -> Result<IndexedPack> {
    let entries = pck_entries(pck)?;
    let sources: BTreeMap<_, _> = entries
        .into_iter()
        .map(|(name, offset, size)| {
            (
                name,
                Source::Pck {
                    path: pck.to_path_buf(),
                    offset,
                    size,
                },
            )
        })
        .collect();
    let mut assets = vec![];
    for (name, source) in &sources {
        if ![
            "walls/",
            "portals/",
            "tilesets/",
            "objects/",
            "terrain/",
            "paths/",
            "patterns/",
            "materials/",
            "roofs/",
            "caves/",
            "borders/",
        ]
            .iter()
            .any(|category| name.starts_with(&format!("res://textures/{category}")))
            || !name.ends_with(".import")
        {
            continue;
        }
        let id = name.trim_end_matches(".import");
        if !is_image(id) {
            continue;
        }
        let text = String::from_utf8(source.read()?)?;
        let remap = text
            .lines()
            .find_map(|line| {
                line.strip_prefix("path=\"")
                    .and_then(|s| s.strip_suffix('"'))
            })
            .context("Godot import has no resource path")?;
        let raw = sources
            .get(remap)
            .with_context(|| format!("Missing Godot resource {remap}"))?;
        let mut f = File::open(pck)?;
        let (offset, size) = if let Source::Pck { offset, size, .. } = raw {
            (*offset, *size)
        } else {
            unreachable!()
        };
        ensure!(size >= 20, "Truncated texture");
        f.seek(SeekFrom::Start(offset))?;
        let mut header = [0; 20];
        f.read_exact(&mut header)?;
        ensure!(
            &header[..4] == b"GDST",
            "Unsupported imported texture {remap}"
        );
        let width = u16::from_le_bytes(header[4..6].try_into()?) as f32;
        let height = u16::from_le_bytes(header[8..10].try_into()?) as f32;
        assets.push(Asset {
            meta: AssetMeta {
                id: id.into(),
                name: Path::new(id)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: id.into(),
                tags: id.split('/').skip(3).take(2).map(str::to_owned).collect(),
                size: Point::new(width / 256., height / 256.),
                pivot: Point::new(0.5, 0.5),
                sha256: String::new(),
                color_mode: if id.starts_with("res://textures/caves/colorable/") {
                    crate::document::ObjectColorMode::Tint
                } else {
                    crate::document::ObjectColorMode::None
                },
            },
            source: Source::Godot(Box::new(raw.clone())),
        });
    }
    if let Some(source) = sources.get("res://data/default.dungeondraft_tags") {
        apply_dd_tags(&mut assets, &source.read()?)?;
    }
    Ok(IndexedPack {
        prefabs: vec![],
        name: "Dungeondraft defaults".into(),
        metadata: PackMetadata::default(),
        path: pck.to_path_buf(),
        assets,
        warnings: vec![],
    })
}
#[derive(Clone, Debug)]
pub struct Asset {
    pub meta: AssetMeta,
    pub source: Source,
}
pub type Library = BTreeMap<String, Asset>;

/// Terrain tiles describe a solid surface. Only their separate border images
/// carry coverage; source tile alpha must not reveal earlier paint or floors.
pub fn prepare_surface(meta: &AssetMeta, image: &mut image::RgbaImage) -> bool {
    if category(meta) != "terrain" {
        return false;
    }
    let mut changed = false;
    for pixel in image.pixels_mut() {
        changed |= pixel[3] != 255;
        pixel[3] = 255;
    }
    changed
}

pub fn decode_surface(asset: &Asset) -> Result<image::RgbaImage> {
    let mut image = decode(&verified_bytes(asset)?)?;
    prepare_surface(&asset.meta, &mut image);
    Ok(image)
}

pub fn category(meta: &AssetMeta) -> &'static str {
    let in_category = |category: &str| {
        meta.id
            .split('/')
            .any(|part| part.eq_ignore_ascii_case(category))
            || meta
            .tags
            .iter()
            .any(|tag| tag.eq_ignore_ascii_case(category))
    };
    // Materials are terrain paint and borders. Floors have their own tileset/pattern categories.
    if in_category("materials") {
        return if meta.name.to_lowercase().contains("_border") {
            "border"
        } else {
            "terrain"
        };
    }
    for category in [
        "portals", "walls", "tilesets", "patterns", "terrain", "paths", "roofs", "caves",
    ] {
        if in_category(category) {
            return if category == "patterns" {
                "tilesets"
            } else {
                category
            };
        }
    }
    for category in [
        "border", "terrain", "portals", "walls", "tilesets", "paths", "roofs",
    ] {
        if meta.tags.iter().any(|t| t.eq_ignore_ascii_case(category)) {
            return category;
        }
    }
    let id = &meta.id;
    let name = meta.name.to_lowercase();
    if name.ends_with("_border") {
        "border"
    } else if name.ends_with("_tile") || id.contains("/terrain/") {
        "terrain"
    } else if id.contains("/portals/") || name.starts_with("door_") || name.starts_with("window_") {
        "portals"
    } else if id.contains("/walls/") {
        "walls"
    } else if id.contains("/tilesets/") || id.contains("/patterns/") {
        "tilesets"
    } else if id.contains("/paths/") {
        "paths"
    } else if id.contains("/roofs/") {
        "roofs"
    } else {
        "objects"
    }
}
pub fn border_for(library: &Library, tile: &str) -> Option<String> {
    let border = tile.replace("_tile.", "_border.");
    (border != tile && library.contains_key(&border)).then_some(border)
}
#[derive(Clone, Debug)]
pub struct IndexedPack {
    pub prefabs: Vec<crate::storage::prefabs::Prefab>,
    pub name: String,
    pub path: PathBuf,
    pub assets: Vec<Asset>,
    pub warnings: Vec<String>,
    pub metadata: PackMetadata,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PackMetadata {
    pub id: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub website: String,
    pub logo: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Fabric {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub pack_version: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub logo: Option<String>,
    pub assets: Vec<AssetMeta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prefabs: Vec<crate::storage::prefabs::Prefab>,
}

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn color_texture_key(asset: &str, color: crate::document::Color) -> String {
    format!(
        "@color:{}:{:02x}{:02x}{:02x}{:02x}",
        hash(asset.as_bytes()),
        color[0],
        color[1],
        color[2],
        color[3]
    )
}
pub fn color_fallback_key(asset: &str) -> String {
    format!("@color-fallback:{}", hash(asset.as_bytes()))
}
pub fn recolor_red_mask(image: &mut image::RgbaImage, color: crate::document::Color) {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a == 0 {
            continue;
        }
        let dominant = r.saturating_sub(g.max(b)) as f32;
        let range = r.saturating_sub(g.min(b)) as f32;
        let hue_distance = if range > 0. {
            60. * (g as f32 - b as f32).abs() / range
        } else {
            60.
        };
        let weight = ((30. - hue_distance) / 10.).clamp(0., 1.);
        let mask = dominant * weight;
        for (index, (channel, target)) in pixel.0[..3].iter_mut().zip(color[..3].iter()).enumerate()
        {
            let base = if index == 0 {
                *channel as f32 - mask
            } else {
                *channel as f32
            };
            *channel = (base + mask * *target as f32 / 255.)
                .round()
                .clamp(0., 255.) as u8;
        }
        pixel.0[3] = ((a as u16 * color[3] as u16 + 127) / 255) as u8;
    }
}
pub fn apply_dd_tags(assets: &mut [Asset], bytes: &[u8]) -> Result<()> {
    ensure!(
        bytes.len() as u64 <= MAX_JSON,
        "Tag file exceeds JSON limit"
    );
    let data: serde_json::Value = serde_json::from_slice(&dd_tag_json(bytes))?;
    let Some(tags) = data
        .get("tags")
        .or_else(|| data.get("Tags"))
        .and_then(|v| v.as_object())
    else {
        return Ok(());
    };
    let mut lookup = BTreeMap::new();
    for (index, asset) in assets.iter().enumerate() {
        lookup.insert(asset.meta.id.clone(), index);
        if let Some(id) = asset.meta.id.strip_prefix("res://") {
            lookup.insert(id.to_owned(), index);
        }
        if let Some(i) = asset.meta.id.find("textures/") {
            lookup.insert(asset.meta.id[i..].to_owned(), index);
            lookup.insert(format!("res://{}", &asset.meta.id[i..]), index);
        }
    }
    for (tag, references) in tags {
        ensure!(tag.len() <= 256, "Tag name exceeds limit");
        let Some(references) = references.as_array() else {
            continue;
        };
        for reference in references.iter().filter_map(|v| v.as_str()) {
            if let Some(index) = lookup.get(reference) {
                let asset = &mut assets[*index];
                if !asset.meta.tags.contains(tag) {
                    asset.meta.tags.push(tag.clone());
                }
                if tag.eq_ignore_ascii_case("Colorable") {
                    asset.meta.color_mode = crate::document::ObjectColorMode::RedMask;
                }
            }
        }
    }
    Ok(())
}
// Godot accepts trailing commas in DD tag dictionaries and arrays. Remove only
// those tokens outside strings; fabric/project JSON keeps its strict parser.
fn dd_tag_json(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut quoted = false;
    let mut escaped = false;
    for (index, &byte) in bytes.iter().enumerate() {
        if quoted {
            normalized.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
            normalized.push(byte);
        } else if byte != b','
            || !bytes[index + 1..]
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_some_and(|b| matches!(b, b']' | b'}'))
        {
            normalized.push(byte);
        }
    }
    normalized
}
pub fn safe_member(name: &str) -> Result<()> {
    ensure!(
        !name.is_empty()
            && name.len() <= 4096
            && !name.contains(['\\', ':', '\0'])
            && !name.starts_with('/'),
        "Unsafe archive path: {name}"
    );
    ensure!(
        name.split('/')
            .all(|s| !s.is_empty() && s != ".." && s != "."),
        "Unsafe archive path: {name}"
    );
    Ok(())
}
pub fn read_zip<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    safe_member(name)?;
    let f = zip
        .by_name(name)
        .with_context(|| format!("Missing archive member {name}"))?;
    ensure!(f.size() <= limit, "Archive member {name} exceeds limit");
    let mut bytes = vec![];
    f.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "Archive member exceeds limit");
    Ok(bytes)
}
pub fn validate_zip<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<()> {
    let mut names = HashSet::new();
    let mut total = 0u64;
    for i in 0..zip.len() {
        let f = zip.by_index(i)?;
        if f.is_dir() {
            continue;
        }
        safe_member(f.name())?;
        ensure!(
            names.insert(f.name().to_string()),
            "Duplicate archive member"
        );
        ensure!(f.size() <= MAX_ENTRY, "Archive member exceeds limit");
        total = total
            .checked_add(f.size())
            .context("Archive size overflow")?;
        ensure!(total <= MAX_TOTAL, "Archive expands beyond 2 GiB");
    }
    Ok(())
}
pub fn decode(bytes: &[u8]) -> Result<image::RgbaImage> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.into_rgba8())
}
pub fn import_image(path: &Path) -> Result<Asset> {
    let bytes = Source::File(path.to_path_buf()).read()?;
    let image = decode(&bytes)?;
    let digest = hash(&bytes);
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let meta = AssetMeta {
        id: format!("image:{digest}"),
        name,
        path: format!(
            "assets/{digest}.{}",
            path.extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
        ),
        tags: vec!["Imported".into()],
        size: Point::new(image.width() as f32 / 256., image.height() as f32 / 256.),
        pivot: Point::new(0.5, 0.5),
        sha256: digest,
        color_mode: crate::document::ObjectColorMode::None,
    };
    Ok(Asset {
        meta,
        source: Source::Bytes(Arc::new(bytes)),
    })
}
pub fn index_pack(path: &Path) -> Result<IndexedPack> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("dungeondraft_pack"))
    {
        return index_pck(path);
    }
    let mut zip = ZipArchive::new(File::open(path)?)?;
    validate_zip(&mut zip)?;
    let prefab_file = zip.file_names().any(|name| name == "prefab.json");
    let manifest: Fabric = serde_json::from_slice(&read_zip(
        &mut zip,
        if prefab_file {
            "prefab.json"
        } else {
            "fabric.json"
        },
        MAX_JSON,
    )?)?;
    ensure!(
        manifest.format
            == (if prefab_file {
                "maploom/prefab"
            } else {
                "maploom/fabric"
            })
            && manifest.version == 1,
        "Unsupported fabric version"
    );
    let mut ids = HashSet::new();
    let mut assets = vec![];
    let mut warnings = vec![];
    let mut metadata = PackMetadata {
        id: manifest.id,
        author: manifest.author,
        version: manifest.pack_version,
        description: manifest.description,
        website: manifest.website,
        logo: None,
    };
    if let Some(logo) = manifest.logo {
        match safe_member(&logo)
            .and_then(|()| read_zip(&mut zip, &logo, 8 * 1024 * 1024))
            .and_then(|b| pack_logo(&b))
        {
            Ok(logo) => metadata.logo = Some(logo),
            Err(e) => warnings.push(format!("Pack logo: {e:#}")),
        }
    }
    for meta in manifest.assets {
        if !ids.insert(meta.id.clone()) {
            continue;
        }
        safe_member(&meta.path)?;
        ensure!(
            meta.size.finite() && meta.size.x > 0. && meta.size.y > 0. && meta.pivot.finite(),
            "Invalid fabric metadata"
        );
        zip.by_name(&meta.path)?;
        assets.push(Asset {
            source: Source::Zip {
                path: path.to_path_buf(),
                entry: meta.path.clone(),
            },
            meta,
        });
    }
    crate::storage::prefabs::validate(&manifest.prefabs, &assets)?;
    Ok(IndexedPack {
        prefabs: manifest.prefabs,
        name: manifest.name,
        path: path.to_path_buf(),
        assets,
        warnings,
        metadata,
    })
}
pub fn index_packs(paths: &[PathBuf]) -> Vec<(PathBuf, Result<IndexedPack>)> {
    paths
        .par_iter()
        .map(|path| (path.clone(), index_pack(path)))
        .collect()
}
fn u32le(f: &mut File) -> Result<u32> {
    let mut b = [0; 4];
    f.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn u64le(f: &mut File) -> Result<u64> {
    let mut b = [0; 8];
    f.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}
pub fn pck_entries(path: &Path) -> Result<Vec<(String, u64, u64)>> {
    let mut f = File::open(path)?;
    let length = f.metadata()?.len();
    ensure!(u32le(&mut f)? == 0x43504447, "Not a Godot PCK archive");
    let version = u32le(&mut f)?;
    ensure!(
        version == 1 || version == 2,
        "Unsupported PCK version {version}"
    );
    for _ in 0..3 {
        u32le(&mut f)?;
    }
    let mut base = 0;
    if version == 2 {
        let flags = u32le(&mut f)?;
        ensure!(flags & 1 == 0, "Encrypted PCK directory is unsupported");
        base = u64le(&mut f)?;
    }
    f.seek(SeekFrom::Current(64))?;
    let count = u32le(&mut f)?;
    let mut entries = vec![];
    for _ in 0..count {
        let n = u32le(&mut f)?;
        ensure!(n > 0 && n <= 4096, "Invalid PCK path length");
        let mut b = vec![0; n as usize];
        f.read_exact(&mut b)?;
        let name = String::from_utf8(b)?.trim_end_matches('\0').to_owned();
        safe_member(name.strip_prefix("res://").unwrap_or(&name))?;
        let offset = u64le(&mut f)?
            .checked_add(base)
            .context("PCK offset overflow")?;
        let size = u64le(&mut f)?;
        f.seek(SeekFrom::Current(16))?;
        if version == 2 {
            ensure!(
                u32le(&mut f)? & 1 == 0,
                "Encrypted PCK member is unsupported"
            );
        }
        ensure!(
            offset.checked_add(size).is_some_and(|end| end <= length),
            "PCK member outside archive"
        );
        entries.push((name, offset, size));
    }
    let data_start = f.stream_position()?;
    let mut spans: Vec<_> = entries
        .iter()
        .filter(|e| e.2 > 0)
        .map(|e| (e.1, e.1 + e.2, e.0.as_str()))
        .collect();
    spans.sort_unstable();
    // Repeated directory records may point at the very same payload.
    spans.dedup();
    ensure!(
        spans.iter().all(|e| e.0 >= data_start) && spans.windows(2).all(|w| w[0].1 <= w[1].0),
        "Overlapping PCK members or invalid offsets"
    );
    let mut names = HashSet::new();
    entries.retain(|(name, _, _)| names.insert(name.clone()));
    Ok(entries)
}
// Image header parsers seek relative to an image, not the containing PCK.
// Keep both reads and seeks inside that entry, without loading its pixel data.
struct PckEntryReader<'a> {
    file: &'a mut File,
    offset: u64,
    size: u64,
    position: u64,
}
impl Read for PckEntryReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let len = (self.size - self.position).min(buffer.len() as u64) as usize;
        let count = self.file.read(&mut buffer[..len])?;
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for PckEntryReader<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let position = match from {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(n) => self.position as i128 + n as i128,
            SeekFrom::End(n) => self.size as i128 + n as i128,
        };
        if !(0..=self.size as i128).contains(&position) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek outside PCK entry",
            ));
        }
        self.file
            .seek(SeekFrom::Start(self.offset + position as u64))?;
        self.position = position as u64;
        Ok(self.position)
    }
}
fn pck_image_size(file: &mut File, offset: u64, size: u64) -> Result<Point> {
    file.seek(SeekFrom::Start(offset))?;
    let entry = PckEntryReader {
        file,
        offset,
        size,
        position: 0,
    };
    let (width, height) = image::ImageReader::new(BufReader::new(entry))
        .with_guessed_format()?
        .into_dimensions()?;
    ensure!(
        width > 0 && height > 0 && width <= 16384 && height <= 16384,
        "Invalid image dimensions"
    );
    Ok(Point::new(width as f32 / 256., height as f32 / 256.))
}
pub fn index_pck(path: &Path) -> Result<IndexedPack> {
    let entries = pck_entries(path)?;
    let mut image_headers = File::open(path)?;
    let mut name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut assets = vec![];
    let mut warnings = vec![];
    let mut tag_sources = vec![];
    let mut metadata = PackMetadata::default();
    let mut preview = None;
    for (entry, offset, size) in &entries {
        if entry.ends_with("/pack.json") && *size <= MAX_JSON {
            let source = Source::Pck {
                path: path.to_path_buf(),
                offset: *offset,
                size: *size,
            };
            match source.read().and_then(|b| {
                Ok(serde_json::from_slice::<serde_json::Value>(&dd_tag_json(
                    &b,
                ))?)
            }) {
                Ok(v) => {
                    let field = |key: &str| {
                        v[key].as_str().map(str::to_owned).unwrap_or_else(|| {
                            if v[key].is_number() {
                                v[key].to_string()
                            } else {
                                String::new()
                            }
                        })
                    };
                    if !field("name").is_empty() {
                        name = field("name");
                    }
                    metadata.id = field("id");
                    metadata.author = field("author");
                    metadata.version = field("version");
                    metadata.description = field("description");
                    metadata.website = field("website");
                    let root = entry.trim_end_matches("pack.json");
                    let logo = ["preview", "logo"]
                        .into_iter()
                        .find_map(|key| v[key].as_str())
                        .unwrap_or("preview.png");
                    preview = Some(if logo.starts_with("res://") {
                        logo.to_owned()
                    } else {
                        format!("{root}{logo}")
                    });
                }
                Err(e) => warnings.push(format!("Pack metadata: {e:#}")),
            }
        }
    }
    if let Some((_, offset, size)) = entries
        .iter()
        .find(|(entry, _, _)| Some(entry) == preview.as_ref())
    {
        let logo = (|| -> Result<String> {
            ensure!(*size <= 8 * 1024 * 1024, "Pack logo exceeds 8 MiB");
            pack_logo(
                &Source::Pck {
                    path: path.to_path_buf(),
                    offset: *offset,
                    size: *size,
                }
                    .read()?,
            )
        })();
        match logo {
            Ok(logo) => metadata.logo = Some(logo),
            Err(e) => warnings.push(format!("Pack logo: {e:#}")),
        }
    }
    for (entry, offset, size) in entries {
        let source = Source::Pck {
            path: path.to_path_buf(),
            offset,
            size,
        };
        if entry.ends_with(".dungeondraft_tags") && size <= MAX_JSON {
            tag_sources.push(source.clone());
        }
        if is_image(&entry) && Some(&entry) != preview.as_ref() {
            if size > MAX_ENTRY {
                warnings.push(format!("Skipped oversized image {entry}"));
                continue;
            }
            let path = Path::new(&entry);
            assets.push(Asset {
                meta: AssetMeta {
                    id: entry.clone(),
                    name: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    path: entry.clone(),
                    tags: entry
                        .split('/')
                        .rev()
                        .skip(1)
                        .take(3)
                        .map(str::to_owned)
                        .collect(),
                    size: Point::new(1., 1.),
                    pivot: Point::new(0.5, 0.5),
                    sha256: String::new(),
                    color_mode: crate::document::ObjectColorMode::None,
                },
                source,
            });
        }
    }
    for source in tag_sources {
        if let Err(error) = apply_dd_tags(&mut assets, &source.read()?) {
            warnings.push(format!("Asset tags: {error:#}"));
        }
    }
    // Paths need their aspect ratio for ribbon UVs. Keep other pack images
    // lazy: reading every header at startup causes random I/O over huge packs.
    // Classify after tags so tag-defined paths receive dimensions too.
    for asset in assets.iter_mut().filter(|a| category(&a.meta) == "paths") {
        if let Source::Pck { offset, size, .. } = &asset.source {
            asset.meta.size =
                pck_image_size(&mut image_headers, *offset, *size).unwrap_or(Point::new(1., 1.));
        }
    }
    Ok(IndexedPack {
        prefabs: vec![],
        name,
        path: path.to_path_buf(),
        assets,
        warnings,
        metadata,
    })
}
fn pack_logo(bytes: &[u8]) -> Result<String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let image = image::DynamicImage::ImageRgba8(decode(bytes)?).thumbnail(256, 256);
    let mut out = std::io::Cursor::new(vec![]);
    image.write_to(&mut out, image::ImageFormat::Png)?;
    Ok(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(out.into_inner())
    ))
}
pub fn is_image(path: &str) -> bool {
    Path::new(path).extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "tif" | "tiff"
        )
    })
}
pub fn verified_bytes(asset: &Asset) -> Result<Vec<u8>> {
    let bytes = asset.source.read()?;
    ensure!(
        asset.meta.sha256.is_empty() || hash(&bytes) == asset.meta.sha256,
        "Asset checksum mismatch: {}",
        asset.meta.name
    );
    Ok(bytes)
}
pub fn atomic_write(path: &Path, write: impl FnOnce(&mut File) -> Result<()>) -> Result<()> {
    // A process-wide single writer also protects library callers outside the UI.
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _writer = WRITER
        .lock()
        .map_err(|_| anyhow::anyhow!("Archive writer was interrupted"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    write(temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    temp.persist(path)
        .map_err(|e| e.error)
        .with_context(|| format!("Could not replace {}", path.display()))?;
    Ok(())
}
pub fn save_fabric(path: &Path, name: &str, author: &str, assets: &[Asset]) -> Result<()> {
    ensure!(!assets.is_empty(), "Fabric needs at least one asset");
    save_fabric_with_prefabs(path, name, author, assets, &[])
}
pub fn save_fabric_with_prefabs(
    path: &Path,
    name: &str,
    author: &str,
    assets: &[Asset],
    prefabs: &[crate::storage::prefabs::Prefab],
) -> Result<()> {
    save_collection(path, name, author, assets, prefabs, false)
}
pub fn save_prefab_file(
    path: &Path,
    name: &str,
    assets: &[Asset],
    prefabs: &[crate::storage::prefabs::Prefab],
) -> Result<()> {
    save_collection(path, name, "", assets, prefabs, true)
}
fn save_collection(
    path: &Path,
    name: &str,
    author: &str,
    assets: &[Asset],
    prefabs: &[crate::storage::prefabs::Prefab],
    prefab_file: bool,
) -> Result<()> {
    crate::storage::prefabs::validate(prefabs, assets)?;
    let mut manifest = Fabric {
        format: if prefab_file {
            "maploom/prefab"
        } else {
            "maploom/fabric"
        }
            .into(),
        version: 1,
        id: id(),
        name: name.into(),
        author: author.into(),
        description: String::new(),
        pack_version: String::new(),
        website: String::new(),
        logo: None,
        assets: vec![],
        prefabs: prefabs.to_vec(),
    };
    atomic_write(path, |file| {
        let mut zip = ZipWriter::new(file);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut written = HashSet::new();
        let mut ids = HashSet::new();
        let mut total = 0u64;
        // Bounded batches avoid retaining an entire large pack in RAM.
        for batch in assets.chunks(4) {
            let blobs = batch
                .par_iter()
                .map(|asset| -> Result<_> {
                    let bytes = verified_bytes(asset)?;
                    decode(&bytes)?;
                    Ok((asset.meta.clone(), bytes))
                })
                .collect::<Result<Vec<_>>>()?;
            for (mut meta, bytes) in blobs {
                ensure!(ids.insert(meta.id.clone()), "Duplicate asset ID");
                let digest = hash(&bytes);
                let format = image::guess_format(&bytes)?;
                let ext = format.extensions_str().first().copied().unwrap_or("png");
                meta.path = format!("images/{digest}.{ext}");
                meta.sha256 = digest;
                if written.insert(meta.path.clone()) {
                    total += bytes.len() as u64;
                    ensure!(total <= MAX_TOTAL, "Fabric exceeds 2 GiB");
                    zip.start_file(&meta.path, options)?;
                    zip.write_all(&bytes)?;
                }
                manifest.assets.push(meta);
            }
        }
        zip.start_file(
            if prefab_file {
                "prefab.json"
            } else {
                "fabric.json"
            },
            options,
        )?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        zip.finish()?;
        Ok(())
    })
}
pub fn crop_asset(
    asset: &Asset,
    rect: [u32; 4],
    flip_x: bool,
    flip_y: bool,
    quarter_turns: u32,
) -> Result<Asset> {
    let mut img = decode(&verified_bytes(asset)?)?;
    let [x, y, w, h] = rect;
    ensure!(
        w > 0
            && h > 0
            && x.checked_add(w).is_some_and(|e| e <= img.width())
            && y.checked_add(h).is_some_and(|e| e <= img.height()),
        "Crop is outside image"
    );
    img = image::imageops::crop_imm(&img, x, y, w, h).to_image();
    if flip_x {
        img = image::imageops::flip_horizontal(&img)
    }
    if flip_y {
        img = image::imageops::flip_vertical(&img)
    }
    for _ in 0..quarter_turns % 4 {
        img = image::imageops::rotate90(&img)
    }
    let mut bytes = std::io::Cursor::new(vec![]);
    img.write_to(&mut bytes, image::ImageFormat::Png)?;
    let bytes = bytes.into_inner();
    let mut meta = asset.meta.clone();
    meta.id = id();
    meta.name = format!("{} variant", meta.name);
    meta.sha256 = hash(&bytes);
    meta.path = format!("images/{}.png", meta.sha256);
    meta.size = Point::new(img.width() as f32 / 256., img.height() as f32 / 256.);
    Ok(Asset {
        meta,
        source: Source::Bytes(Arc::new(bytes)),
    })
}
pub fn require_assets(
    project: &crate::document::Project,
    level: usize,
    library: &Library,
) -> Result<()> {
    if let Some(id) = &project.background_asset {
        ensure!(library.contains_key(id), "Missing base terrain {id}");
    }
    for layer in &project.levels[level].layers {
        if !layer.visible {
            continue;
        }
        for item in &layer.items {
            if matches!(
                item.kind,
                crate::document::ItemKind::Floor { .. }
                    | crate::document::ItemKind::Roof { .. }
                    | crate::document::ItemKind::Wall { .. }
                    | crate::document::ItemKind::Path { .. }
                    | crate::document::ItemKind::Terrain { .. }
                    | crate::document::ItemKind::Portal { .. }
            ) {
                ensure!(
                    item.texture.is_some(),
                    "{} has no asset texture. Choose an asset before exporting.",
                    item.name
                );
            }
            for asset in item.asset_ids() {
                if !library.contains_key(asset) {
                    bail!("Missing asset {asset}. Load the required pack before exporting.")
                }
            }
        }
    }
    Ok(())
}
pub fn default_fabric_path() -> Option<PathBuf> {
    crate::resources::directories()
        .into_iter()
        .map(|directory| directory.join("resources/defaults.mloom_fabric"))
        .find(|path| path.is_file())
}
pub fn load_defaults(library: &mut Library) -> Result<()> {
    if let Some(path) = default_fabric_path() {
        for asset in index_pack(&path)?.assets {
            library.entry(asset.meta.id.clone()).or_insert(asset);
        }
    }
    Ok(())
}
