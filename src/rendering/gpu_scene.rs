//! Persistent scene preparation for the WebGL renderer. No raster readback or SVG.
use crate::{
    assets::{self, Asset, Library, Source},
    document::*,
    render::{self, SceneTextures, View},
};
use anyhow::{ensure, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use egui::{Color32, Pos2, Rect, TextureHandle, TextureId, Vec2};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    hash::{Hash, Hasher},
    io::Cursor,
    sync::Arc,
    time::Instant,
};

const SCENE_SCALE: f32 = 64.;
const CACHE_BUDGET: usize = 192 * 1024 * 1024;
const MAX_TEXTURE_SIDE: u32 = 4096;
#[derive(Clone, Serialize)]
pub struct TextureUpload {
    pub key: String,
    pub width: usize,
    pub height: usize,
    pub data: String,
    pub format: String,
    pub repeat: bool,
    pub premultiplied: bool,
    pub alias: Option<String>,
}
#[derive(Serialize)]
pub struct GpuMesh {
    pub key: String,
    pub item: Option<String>,
    pub texture: String,
    pub mode: u8,
    pub clip: [f32; 4],
    pub vertices: String,
    pub indices: String,
    pub vertex_count: usize,
    pub index_count: usize,
}
#[derive(Default, Serialize)]
pub struct SceneStats {
    pub build_ms: f64,
    pub decoded_images: usize,
    pub cache_hits: usize,
    pub vertices: usize,
    pub texture_bytes: usize,
}
#[derive(Serialize)]
pub struct GpuScene {
    pub underlay_meshes: usize,
    pub generation: String,
    pub revision: u64,
    pub width: u32,
    pub height: u32,
    pub meshes: Vec<GpuMesh>,
    pub textures: Vec<TextureUpload>,
    pub used_textures: Vec<String>,
    pub text_bounds: BTreeMap<String, crate::text::TextBounds>,
    pub stats: SceneStats,
}
pub struct SceneOptions {
    pub level: usize,
    pub grid: bool,
    pub lights: bool,
    pub revision: u64,
    pub known_meshes: HashSet<String>,
}
struct CachedAsset {
    handle: TextureHandle,
    mask: Option<TextureHandle>,
    fingerprint: u64,
    original_size: Vec2,
    bytes: usize,
    touched: u64,
    light_boundary: Option<Vec<[Point; 2]>>,
}
struct FontTexture {
    width: usize,
    height: usize,
    pixels: Vec<Color32>,
    version: u64,
}
pub struct SceneRenderer {
    ctx: egui::Context,
    generation: String,
    frame: u64,
    assets: BTreeMap<String, CachedAsset>,
    uploads: BTreeMap<TextureId, TextureUpload>,
    fonts: BTreeMap<TextureId, FontTexture>,
    local_fonts: crate::text::FontBook,
}
impl Default for SceneRenderer {
    fn default() -> Self {
        let ctx = egui::Context::default();
        ctx.input_mut(|input| input.max_texture_side = MAX_TEXTURE_SIDE as usize);
        Self {
            ctx,
            generation: id(),
            frame: 0,
            assets: BTreeMap::new(),
            uploads: BTreeMap::new(),
            fonts: BTreeMap::new(),
            local_fonts: Default::default(),
        }
    }
}
fn source_hash(source: &Source, hasher: &mut impl Hasher) {
    std::mem::discriminant(source).hash(hasher);
    let path = match source {
        Source::Catalog { id, source } => {
            id.hash(hasher);
            source_hash(source, hasher);
            None
        }
        Source::File(path) => Some(path),
        Source::Zip { path, entry } => {
            entry.hash(hasher);
            Some(path)
        }
        Source::Pck { path, offset, size } => {
            offset.hash(hasher);
            size.hash(hasher);
            Some(path)
        }
        Source::Bytes(bytes) => {
            (Arc::as_ptr(bytes) as usize).hash(hasher);
            bytes.len().hash(hasher);
            None
        }
        Source::Godot(source) => {
            source_hash(source, hasher);
            None
        }
    };
    if let Some(path) = path {
        path.hash(hasher);
        if let Ok(meta) = std::fs::metadata(path) {
            meta.len().hash(hasher);
            meta.modified().ok().hash(hasher);
        }
    }
}
fn fingerprint(asset: &Asset) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    asset.meta.sha256.hash(&mut h);
    source_hash(&asset.source, &mut h);
    h.finish()
}
fn texture_id(id: TextureId) -> String {
    match id {
        TextureId::Managed(n) => format!("m{n}"),
        TextureId::User(n) => format!("u{n}"),
    }
}
impl SceneRenderer {
    fn drop_asset(&mut self, id: &str) {
        if let Some(a) = self.assets.remove(id) {
            self.uploads.remove(&a.handle.id());
            if let Some(mask) = a.mask {
                self.uploads.remove(&mask.id());
            }
        }
    }
    fn load(&mut self, asset: &Asset, blocks_light: bool, stats: &mut SceneStats) -> Result<()> {
        let hash = fingerprint(asset);
        if self
            .assets
            .get(&asset.meta.id)
            .is_some_and(|a| a.fingerprint == hash)
        {
            let cached = self.assets.get_mut(&asset.meta.id).unwrap();
            if blocks_light && cached.light_boundary.is_none() {
                cached.light_boundary =
                    Some(assets::alpha_boundary(&assets::decode_surface(asset)?));
                stats.decoded_images += 1;
            }
            cached.touched = self.frame;
            stats.cache_hits += 1;
            return Ok(());
        }
        self.drop_asset(&asset.meta.id);
        let original = assets::verified_bytes(asset)?;
        let mut image = assets::decode(&original)?;
        let surface_changed = assets::prepare_surface(&asset.meta, &mut image);
        let light_boundary = blocks_light.then(|| assets::alpha_boundary(&image));
        let original_size = Vec2::new(image.width() as f32, image.height() as f32);
        let format = image::guess_format(&original)?;
        let resized = image.width() > MAX_TEXTURE_SIDE || image.height() > MAX_TEXTURE_SIDE;
        if resized {
            image = image::DynamicImage::ImageRgba8(image)
                .thumbnail(MAX_TEXTURE_SIDE, MAX_TEXTURE_SIDE)
                .into_rgba8();
        }
        let size = [image.width() as usize, image.height() as usize];
        let pixels = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let handle =
            self.ctx
                .load_texture(&asset.meta.id, pixels, egui::TextureOptions::LINEAR_REPEAT);
        let (bytes, mime) = if !resized
            && !surface_changed
            && matches!(
                format,
                image::ImageFormat::Png
                    | image::ImageFormat::Jpeg
                    | image::ImageFormat::WebP
                    | image::ImageFormat::Gif
                    | image::ImageFormat::Bmp
            ) {
            (original, format.to_mime_type().to_string())
        } else {
            let mut png = Cursor::new(vec![]);
            image.write_to(&mut png, image::ImageFormat::Png)?;
            (png.into_inner(), "image/png".into())
        };
        let key = format!("{}:{}", self.generation, texture_id(handle.id()));
        self.uploads.insert(
            handle.id(),
            TextureUpload {
                key,
                width: size[0],
                height: size[1],
                data: STANDARD.encode(bytes),
                format: mime,
                repeat: true,
                premultiplied: false,
                alias: None,
            },
        );
        self.assets.insert(
            asset.meta.id.clone(),
            CachedAsset {
                handle,
                mask: None,
                fingerprint: hash,
                original_size,
                bytes: size[0] * size[1] * 4,
                touched: self.frame,
                light_boundary,
            },
        );
        stats.decoded_images += 1;
        Ok(())
    }
    fn mask_handle(&mut self, id: &str) -> Result<TextureHandle> {
        if let Some(mask) = self.assets[id].mask.as_ref() {
            return Ok(mask.clone());
        }
        // A logical shader binding aliases the same browser GPU texture. Only
        // dimensions are needed here; a tiny constant image creates that binding.
        let mask = self.ctx.load_texture(
            format!("mask:{id}"),
            egui::ColorImage::new([1, 1], vec![Color32::WHITE]),
            egui::TextureOptions::LINEAR,
        );
        let base = &self.uploads[&self.assets[id].handle.id()];
        self.uploads.insert(
            mask.id(),
            TextureUpload {
                key: format!("{}:{}", self.generation, texture_id(mask.id())),
                width: base.width,
                height: base.height,
                data: String::new(),
                format: "alias".into(),
                repeat: base.repeat,
                premultiplied: false,
                alias: Some(base.key.clone()),
            },
        );
        self.assets
            .get_mut(id)
            .context("Missing shader asset")?
            .mask = Some(mask.clone());
        Ok(mask)
    }
    pub fn scene(
        &mut self,
        project: &Project,
        library: &Library,
        options: SceneOptions,
        known: &HashSet<String>,
    ) -> Result<GpuScene> {
        project.validate()?;
        ensure!(options.level < project.levels.len(), "Invalid level");
        let start_level = if options.level > project.main_level_index() {
            0
        } else {
            options.level
        };
        self.local_fonts.prepare(
            &self.ctx,
            project.levels[start_level..=options.level]
                .iter()
                .flat_map(|l| &l.layers)
                .flat_map(|l| &l.items),
        );
        let mut underlay = Vec::new();
        let mut textures = Vec::new();
        let mut used = HashSet::new();
        let mut stats = SceneStats::default();
        for level in start_level..options.level {
            let scene = self.scene_flat(
                &project.floor_surface(level),
                library,
                SceneOptions {
                    level: 0,
                    grid: false,
                    lights: options.lights,
                    revision: options.revision,
                    known_meshes: options.known_meshes.clone(),
                },
                known,
            )?;
            underlay.extend(scene.meshes);
            textures.extend(scene.textures);
            used.extend(scene.used_textures);
            stats.build_ms += scene.stats.build_ms;
            stats.decoded_images += scene.stats.decoded_images;
            stats.cache_hits += scene.stats.cache_hits;
            stats.vertices += scene.stats.vertices;
        }
        let mut scene = self.scene_flat(
            &project.floor_surface(options.level),
            library,
            SceneOptions {
                level: 0,
                ..options
            },
            known,
        )?;
        scene.underlay_meshes = underlay.len();
        underlay.append(&mut scene.meshes);
        scene.meshes = underlay;
        textures.append(&mut scene.textures);
        let mut keys = HashSet::new();
        textures.retain(|t| keys.insert(t.key.clone()));
        scene.textures = textures;
        used.extend(scene.used_textures);
        scene.used_textures = used.into_iter().collect();
        scene.stats.build_ms += stats.build_ms;
        scene.stats.decoded_images += stats.decoded_images;
        scene.stats.cache_hits += stats.cache_hits;
        scene.stats.vertices += stats.vertices;
        Ok(scene)
    }

    fn scene_flat(
        &mut self,
        project: &Project,
        library: &Library,
        options: SceneOptions,
        known: &HashSet<String>,
    ) -> Result<GpuScene> {
        let SceneOptions {
            level,
            grid,
            lights,
            revision,
            known_meshes,
        } = options;
        let start = Instant::now();
        let mut stats = SceneStats::default();
        let mut project = crate::storage::prefabs::expanded(project).into_owned();
        crate::formats::resolve_dd_assets(&mut project, library)?;
        project.validate()?;
        ensure!(level < project.levels.len(), "Invalid level");
        assets::require_assets(&project, level, library)?;
        self.frame += 1;
        self.local_fonts.prepare(
            &self.ctx,
            project.levels[level].layers.iter().flat_map(|l| &l.items),
        );
        let mut needed = HashSet::new();
        if let Some(id) = &project.background_asset {
            needed.insert(id.clone());
        }
        for item in project.levels[level]
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
        {
            needed.extend(item.asset_ids().into_iter().map(str::to_owned));
        }
        let blocking_assets: HashSet<_> = project.levels[level]
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
            .filter(|item| item.blocks_light)
            .filter_map(|item| {
                if let ItemKind::Object { asset, .. } = &item.kind {
                    Some(asset.as_str())
                } else {
                    None
                }
            })
            .collect();
        for id in &needed {
            self.load(
                &library[id],
                lights && blocking_assets.contains(id.as_str()),
                &mut stats,
            )?;
        }
        let visible_bytes: usize = needed.iter().map(|id| self.assets[id].bytes).sum();
        ensure!(
            visible_bytes <= CACHE_BUDGET,
            "Visible textures exceed the 192 MiB preview budget"
        );
        let mut cached_bytes: usize = self.assets.values().map(|a| a.bytes).sum();
        let mut cold: Vec<_> = self
            .assets
            .iter()
            .filter(|(id, _)| !needed.contains(*id))
            .map(|(id, a)| (a.touched, id.clone(), a.bytes))
            .collect();
        cold.sort();
        for (_, id, bytes) in cold {
            if cached_bytes <= CACHE_BUDGET {
                break;
            }
            self.drop_asset(&id);
            cached_bytes -= bytes;
        }
        stats.texture_bytes = cached_bytes;
        let mut textures: BTreeMap<_, _> = needed
            .iter()
            .map(|id| (id.clone(), self.assets[id].handle.clone()))
            .collect();
        let sizes: BTreeMap<_, _> = needed
            .iter()
            .map(|id| (id.clone(), self.assets[id].original_size))
            .collect();
        let modes: BTreeMap<_, _> = needed
            .iter()
            .map(|id| (id.clone(), library[id].meta.object_color_mode()))
            .collect();
        let mut masked = HashSet::new();
        for item in project.levels[level]
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
        {
            if let ItemKind::Object { asset, .. } = &item.kind {
                if modes[asset] == ObjectColorMode::RedMask {
                    let handle = self.mask_handle(asset)?;
                    masked.insert(handle.id());
                    textures.insert(assets::color_texture_key(asset, item.tint), handle);
                }
            }
        }
        let rect = Rect::from_min_size(
            Pos2::ZERO,
            Vec2::new(project.width as f32, project.height as f32) * SCENE_SCALE,
        );
        let blockers = crate::light::Blockers::with_boundaries(&project, level, library, |id| {
            self.assets
                .get(id)
                .and_then(|a| a.light_boundary.as_deref())
        });
        let mut item_shapes = Vec::new();
        let output = self.ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ctx| {
                item_shapes = render::draw_scene_gpu(
                    &ctx.layer_painter(egui::LayerId::background()),
                    &project,
                    level,
                    &SceneTextures {
                        textures: &textures,
                        modes: &modes,
                        sizes: Some(&sizes),
                        shader_masks: true,
                        blockers: Some(&blockers),
                    },
                    &View {
                        origin: Pos2::ZERO,
                        scale: SCENE_SCALE,
                        grid,
                        lights,
                    },
                );
            },
        );
        for (id, delta) in output.textures_delta.set {
            if self.uploads.contains_key(&id) && !self.fonts.contains_key(&id) {
                continue;
            }
            let egui::ImageData::Color(image) = delta.image;
            let entry = self.fonts.entry(id).or_insert_with(|| FontTexture {
                width: 0,
                height: 0,
                pixels: vec![],
                version: 0,
            });
            if let Some([x, y]) = delta.pos {
                for row in 0..image.size[1] {
                    let dst = (y + row) * entry.width + x;
                    entry.pixels[dst..dst + image.size[0]].copy_from_slice(
                        &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                    );
                }
            } else {
                entry.width = image.size[0];
                entry.height = image.size[1];
                entry.pixels = image.pixels.clone();
            }
            entry.version += 1;
            self.uploads.insert(
                id,
                TextureUpload {
                    key: format!("{}:{}:f{}", self.generation, texture_id(id), entry.version),
                    width: entry.width,
                    height: entry.height,
                    data: STANDARD.encode(
                        entry
                            .pixels
                            .iter()
                            .flat_map(|p| p.to_array())
                            .collect::<Vec<_>>(),
                    ),
                    format: "rgba".into(),
                    repeat: false,
                    premultiplied: true,
                    alias: None,
                },
            );
        }
        let mut meshes = vec![];
        let mut used = HashSet::new();
        let light_textures = crate::light::texture_ids(&self.ctx);
        // Tessellate each owner's span separately so egui cannot merge two
        // objects (or the background/grid) into an indistinguishable mesh.
        let mut owners = vec![None; output.shapes.len()];
        for span in item_shapes {
            for owner in &mut owners[span.range] {
                *owner = Some(span.item.split(":part:").next().unwrap().to_owned());
            }
        }
        let mut groups: Vec<(Option<String>, Vec<egui::epaint::ClippedShape>)> = Vec::new();
        for (shape, owner) in output.shapes.into_iter().zip(owners) {
            if groups.last().is_none_or(|(previous, _)| *previous != owner) {
                groups.push((owner.clone(), Vec::new()));
            }
            groups.last_mut().unwrap().1.push(shape);
        }
        for (owner, shapes) in groups {
            for primitive in self.ctx.tessellate(shapes, 1.) {
                let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else {
                    continue;
                };
                let upload = self
                    .uploads
                    .get(&mesh.texture_id)
                    .context("Missing GPU texture")?;
                used.insert(upload.key.clone());
                if let Some(alias) = &upload.alias {
                    used.insert(alias.clone());
                }
                let mut vertices = Vec::with_capacity(mesh.vertices.len() * 48);
                for v in &mesh.vertices {
                    let mut c = v.color.to_array();
                    let tint = v.color.to_srgba_unmultiplied();
                    let ambient = if lights && !light_textures.contains(&mesh.texture_id) {
                        project.ambient_light
                    } else {
                        [255; 4]
                    };
                    if masked.contains(&mesh.texture_id) {
                        // Mask recoloring uses `tint`; `color` carries ambient light.
                        c = ambient;
                    } else {
                        for channel in 0..3 {
                            c[channel] =
                                ((c[channel] as u32 * ambient[channel] as u32 + 127) / 255) as u8;
                        }
                    }
                    for f in [
                        v.pos.x / SCENE_SCALE,
                        v.pos.y / SCENE_SCALE,
                        v.uv.x,
                        v.uv.y,
                        c[0] as f32 / 255.,
                        c[1] as f32 / 255.,
                        c[2] as f32 / 255.,
                        c[3] as f32 / 255.,
                        tint[0] as f32 / 255.,
                        tint[1] as f32 / 255.,
                        tint[2] as f32 / 255.,
                        tint[3] as f32 / 255.,
                    ] {
                        vertices.extend_from_slice(&f.to_le_bytes());
                    }
                }
                stats.vertices += mesh.vertices.len();
                let indices: Vec<_> = mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
                let clip = [
                    primitive.clip_rect.min.x / SCENE_SCALE,
                    primitive.clip_rect.min.y / SCENE_SCALE,
                    primitive.clip_rect.max.x / SCENE_SCALE,
                    primitive.clip_rect.max.y / SCENE_SCALE,
                ];
                let mode = u8::from(masked.contains(&mesh.texture_id));
                let mut hash = Sha256::new();
                hash.update(upload.key.as_bytes());
                hash.update([mode]);
                for f in clip {
                    hash.update(f.to_le_bytes());
                }
                hash.update(&vertices);
                hash.update(&indices);
                let key = format!("{:x}", hash.finalize());
                let known = known_meshes.contains(&key);
                meshes.push(GpuMesh {
                    key,
                    item: owner.clone(),
                    texture: upload.key.clone(),
                    mode,
                    clip,
                    vertices: if known {
                        String::new()
                    } else {
                        STANDARD.encode(vertices)
                    },
                    indices: if known {
                        String::new()
                    } else {
                        STANDARD.encode(indices)
                    },
                    vertex_count: mesh.vertices.len(),
                    index_count: mesh.indices.len(),
                });
            }
        }
        let transfers = self
            .uploads
            .values()
            .filter(|u| used.contains(&u.key) && !known.contains(&u.key))
            .cloned()
            .collect();
        for id in output.textures_delta.free {
            self.uploads.remove(&id);
            self.fonts.remove(&id);
        }
        stats.build_ms = start.elapsed().as_secs_f64() * 1000.;
        Ok(GpuScene {
            underlay_meshes: 0,
            generation: self.generation.clone(),
            revision,
            width: project.width,
            height: project.height,
            meshes,
            textures: transfers,
            used_textures: used.into_iter().collect(),
            text_bounds: project.levels[level]
                .layers
                .iter()
                .filter(|l| l.visible)
                .flat_map(|l| &l.items)
                .filter(|i| matches!(i.kind, ItemKind::Text { .. }))
                .map(|i| {
                    (
                        i.id.clone(),
                        crate::text::bounds(i, project.pixels_per_grid as f32),
                    )
                })
                .collect(),
            stats,
        })
    }
}
