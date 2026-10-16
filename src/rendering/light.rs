//! Shared light stamps for the live scene, raster exports and SVG.
use crate::{
    assets::Library,
    document::{ItemKind, Point, PortalType, Project},
};
use egui::{Color32, Pos2, Rect, TextureHandle};
use image::{Rgba, RgbaImage};
use std::collections::VecDeque;

/// Visible walls have their portal intervals removed; doors restore an opaque
/// segment, while windows leave the opening transparent, including legacy packs.
pub struct Blockers(Vec<[Point; 2]>);

impl Blockers {
    pub fn new(project: &Project, level: usize, library: &Library) -> Self {
        let expanded = crate::storage::prefabs::expanded(project);
        let project = expanded.as_ref();
        let boundaries = crate::formats::object_boundaries(&project.levels[level], library);
        Self::with_boundaries(project, level, library, |id| {
            boundaries.get(id).map(Vec::as_slice)
        })
    }

    pub(crate) fn with_boundaries<'a>(
        project: &Project,
        level: usize,
        library: &Library,
        boundary: impl Fn(&str) -> Option<&'a [[Point; 2]]>,
    ) -> Self {
        let level = &project.levels[level];
        let mut segments: Vec<_> = crate::formats::line_of_sight_with_boundaries(level, boundary)
            .into_iter()
            .map(|line| [line[0], line[1]])
            .collect();
        for item in level
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
        {
            if let ItemKind::Portal { bounds, .. } = &item.kind {
                let meta = item.texture.as_ref().and_then(|id| {
                    library
                        .get(id)
                        .map(|asset| &asset.meta)
                        .or_else(|| project.assets.get(id))
                });
                if item.portal_type_with_asset(meta) == PortalType::Door {
                    segments.push(*bounds);
                }
            }
        }
        segments.retain(|[a, b]| a.distance(*b) > 1e-6);
        Self(segments)
    }

    /// Cast rays on either side of each endpoint and intersection. Clipping the
    /// segments to the light's square first also includes walls with both ends
    /// outside the radius. All intersections use f64 to keep thin gaps stable.
    pub fn visibility(&self, origin: Point, range: f32) -> Option<Vec<Point>> {
        let range = range as f64;
        let local = |p: Point| [p.x as f64 - origin.x as f64, p.y as f64 - origin.y as f64];
        let mut segments: Vec<_> = self
            .0
            .iter()
            .filter_map(|[a, b]| clip([local(*a), local(*b)], range))
            .collect();
        if segments.is_empty() {
            return None;
        }
        let mut events: Vec<_> = segments.iter().flat_map(|line| *line).collect();
        for (i, a) in segments.iter().enumerate() {
            for b in &segments[i + 1..] {
                if let Some(p) = crossing(*a, *b) {
                    events.push(p);
                }
            }
        }
        let corners = [
            [-range, -range],
            [range, -range],
            [range, range],
            [-range, range],
        ];
        events.extend(corners);
        segments.extend((0..4).map(|i| [corners[i], corners[(i + 1) % 4]]));
        let mut angles: Vec<_> = events
            .into_iter()
            .flat_map(|p| {
                let angle = p[1].atan2(p[0]);
                [angle - 1e-7, angle, angle + 1e-7]
            })
            .collect();
        angles.sort_by(f64::total_cmp);
        angles.dedup_by(|a, b| (*a - *b).abs() < 1e-10);
        Some(
            angles
                .into_iter()
                .map(|angle| {
                    let dir = [angle.cos(), angle.sin()];
                    let mut distance = range * 2.;
                    for [a, b] in &segments {
                        let edge = sub(*b, *a);
                        let denominator = cross(dir, edge);
                        if denominator.abs() < 1e-12 {
                            continue;
                        }
                        let t = cross(*a, edge) / denominator;
                        let u = cross(*a, dir) / denominator;
                        if t > 1e-7 && (-1e-9..=1. + 1e-9).contains(&u) {
                            distance = distance.min(t);
                        }
                    }
                    Point::new(
                        (origin.x as f64 + dir[0] * distance) as f32,
                        (origin.y as f64 + dir[1] * distance) as f32,
                    )
                })
                .collect(),
        )
    }
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn cross(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn crossing([a, b]: [[f64; 2]; 2], [c, d]: [[f64; 2]; 2]) -> Option<[f64; 2]> {
    let ab = sub(b, a);
    let cd = sub(d, c);
    let den = cross(ab, cd);
    if den.abs() < 1e-12 {
        return None;
    }
    let ac = sub(c, a);
    let t = cross(ac, cd) / den;
    let u = cross(ac, ab) / den;
    if !(0.0..=1.).contains(&t) || !(0.0..=1.).contains(&u) {
        return None;
    }
    Some([a[0] + ab[0] * t, a[1] + ab[1] * t])
}

fn clip([a, b]: [[f64; 2]; 2], range: f64) -> Option<[[f64; 2]; 2]> {
    let delta = sub(b, a);
    let (mut lo, mut hi) = (0_f64, 1_f64);
    for axis in 0..2 {
        if delta[axis].abs() < 1e-12 {
            if a[axis].abs() > range {
                return None;
            }
        } else {
            let t0 = (-range - a[axis]) / delta[axis];
            let t1 = (range - a[axis]) / delta[axis];
            lo = lo.max(t0.min(t1));
            hi = hi.min(t0.max(t1));
        }
    }
    if hi <= lo {
        return None;
    }
    Some([
        [a[0] + delta[0] * lo, a[1] + delta[1] * lo],
        [a[0] + delta[0] * hi, a[1] + delta[1] * hi],
    ])
}

pub fn default_smoothing() -> f32 {
    1.
}

pub fn texture_ids(ctx: &egui::Context) -> std::collections::HashSet<egui::TextureId> {
    ctx.data_mut(|data| {
        data.get_temp::<std::collections::HashSet<egui::TextureId>>(egui::Id::new(
            "scene-light-textures",
        ))
            .unwrap_or_default()
    })
}

pub fn begin_scene(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.insert_temp(
            egui::Id::new("scene-light-textures"),
            std::collections::HashSet::<egui::TextureId>::new(),
        )
    });
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

// Piecewise-linear angular noise gives the unsmoothed bands irregular,
// angular edges. Its period closes at 2 PI and does not change on repaint.
fn contour(angle: f32, band: usize) -> f32 {
    const SEGMENTS: u32 = 80;
    let a = angle.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU * SEGMENTS as f32;
    let noise = |index: u32| {
        let mut n = (index % SEGMENTS).wrapping_add(band as u32 * 137 + 31);
        n = (n ^ (n >> 16)).wrapping_mul(0x45d9f3b);
        n = (n ^ (n >> 16)).wrapping_mul(0x45d9f3b);
        (n ^ (n >> 16)) as f32 / u32::MAX as f32
    };
    noise(a.floor() as u32) * (1. - a.fract()) + noise(a.floor() as u32 + 1) * a.fract()
}

pub fn rasterize(smoothing: f32, intensity: f32, side: u32) -> RgbaImage {
    let side = side.clamp(32, 2048);
    let smoothing = smoothing.clamp(0., 1.);
    // Differences between the five nested bands, from the centre outward.
    let weights = [0.25, 0.19, 0.14, 0.085, 0.055];
    RgbaImage::from_fn(side, side, |x, y| {
        let x = (x as f32 + 0.5) / side as f32 * 2. - 1.;
        let y = (y as f32 + 0.5) / side as f32 * 2. - 1.;
        let radius = x.hypot(y);
        if radius >= 1. || intensity <= 0. {
            return Rgba([255, 255, 255, 0]);
        }
        let angle = y.atan2(x);
        let mut banded = 0.;
        for (band, weight) in weights.iter().enumerate() {
            let edge =
                (band + 1) as f32 / 5. * (1. - 0.12 * (1. - smoothing) * contour(angle, band));
            let coverage = if smoothing == 0. {
                if radius < edge {
                    1.
                } else {
                    0.
                }
            } else {
                smoothstep((edge - radius) / (0.22 * smoothing))
            };
            banded += weight * coverage;
        }
        let soft = 0.72 * (-4. * radius * radius).exp() * (1. - smoothstep((radius - 0.75) / 0.25));
        let alpha = banded * (1. - smoothing * smoothing) + soft * smoothing * smoothing;
        // Optical density keeps the whole intensity range useful and makes
        // intensity zero fully transparent, including the source centre.
        let alpha = (1. - (1. - alpha).powf(intensity)) * 255.;
        Rgba([255, 255, 255, alpha.round() as u8])
    })
}

#[derive(Clone, Default)]
struct Cache(VecDeque<((u32, u32), TextureHandle)>);

pub struct Stamp {
    pub position: Point,
    pub range: f32,
    pub smoothing: f32,
    pub intensity: f32,
    pub tint: Color32,
}

pub fn draw(
    painter: &egui::Painter,
    view: &crate::render::View,
    stamp: Stamp,
    visibility: Option<&[Point]>,
) {
    let Stamp {
        position,
        range,
        smoothing,
        intensity,
        tint,
    } = stamp;
    if intensity <= 0. || tint.a() == 0 {
        return;
    }
    let key = (smoothing.to_bits(), intensity.to_bits());
    let cache_id = egui::Id::new("light-stamps");
    let cached = painter.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(cache_id);
        let index = cache.0.iter().position(|entry| entry.0 == key)?;
        let entry = cache.0.remove(index).unwrap();
        let texture = entry.1.clone();
        cache.0.push_back(entry);
        Some(texture)
    });
    let texture = cached.unwrap_or_else(|| {
        let image = rasterize(smoothing, intensity, 512);
        let image = egui::ColorImage::from_rgba_unmultiplied([512, 512], image.as_raw());
        let texture = painter.ctx().load_texture(
            format!("light:{key:?}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        painter.ctx().data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<Cache>(cache_id);
            // At most 16 MiB of stamps, shared across positions, radii and colors.
            if cache.0.len() >= 16 {
                cache.0.pop_front();
            }
            cache.0.push_back((key, texture.clone()));
        });
        texture
    });
    painter.ctx().data_mut(|data| {
        data.get_temp_mut_or_default::<std::collections::HashSet<egui::TextureId>>(egui::Id::new(
            "scene-light-textures",
        ))
            .insert(texture.id());
    });
    if let Some(polygon) = visibility {
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for point in std::iter::once(&position).chain(polygon) {
            mesh.vertices.push(egui::epaint::Vertex {
                pos: view.screen(*point),
                uv: egui::pos2(
                    0.5 + (point.x - position.x) / (2. * range),
                    0.5 + (point.y - position.y) / (2. * range),
                ),
                color: tint,
            });
        }
        for i in 0..polygon.len() {
            mesh.add_triangle(0, i as u32 + 1, ((i + 1) % polygon.len()) as u32 + 1);
        }
        painter.add(egui::Shape::mesh(mesh));
        return;
    }
    painter.image(
        texture.id(),
        Rect::from_min_max(
            view.screen(Point::new(position.x - range, position.y - range)),
            view.screen(Point::new(position.x + range, position.y + range)),
        ),
        Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)),
        tint,
    );
}
