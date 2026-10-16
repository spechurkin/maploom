//! A procedural water material shared by the editor, GPU exports and SVG.
//! Shore distance is measured over the entire region, including island shores.
use crate::document::{Point, WaterStyle};
use egui::{Color32, Rect, TextureHandle};
use image::{Rgba, RgbaImage};
use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
};

pub struct WaterImage {
    pub origin: Point,
    pub size: Point,
    pub image: RgbaImage,
}

pub fn rasterize(rings: &[Vec<Point>], style: &WaterStyle, pixels_per_grid: f32) -> WaterImage {
    let mut min = Point::new(f32::INFINITY, f32::INFINITY);
    let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
    for p in rings.iter().flatten() {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    let span = (max.x - min.x).max(max.y - min.y);
    if !span.is_finite() || span <= 0. {
        return WaterImage {
            origin: Point::default(),
            size: Point::new(1., 1.),
            image: RgbaImage::new(1, 1),
        };
    }
    let density = pixels_per_grid.clamp(1., 256.).min(2044. / span);
    let step = 1. / density;
    min.x -= step * 2.;
    min.y -= step * 2.;
    let w = ((max.x - min.x) * density).ceil() as usize + 2;
    let h = ((max.y - min.y) * density).ceil() as usize + 2;
    let mut distance = vec![0_f32; w * h];
    // Even-odd scan conversion preserves holes and separate components without
    // testing every pixel against every edge of a long brush stroke.
    for row in 0..h {
        let y = min.y + (row as f32 + 0.5) * step;
        let mut intersections = Vec::new();
        for ring in rings {
            for (a, b) in ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .take(ring.len())
            {
                if (a.y > y) != (b.y > y) {
                    intersections.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                }
            }
        }
        intersections.sort_by(f32::total_cmp);
        for pair in intersections.chunks_exact(2) {
            let left =
                (((pair[0] - min.x) * density - 0.5).ceil() as isize).clamp(0, w as isize) as usize;
            let right =
                (((pair[1] - min.x) * density - 0.5).ceil() as isize).clamp(0, w as isize) as usize;
            distance[row * w + left..row * w + right].fill(1e8);
        }
    }
    let diagonal = std::f32::consts::SQRT_2;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            distance[i] = distance[i]
                .min(distance[i - 1] + 1.)
                .min(distance[i - w] + 1.)
                .min(distance[i - w - 1] + diagonal)
                .min(distance[i - w + 1] + diagonal);
        }
    }
    for y in (1..h - 1).rev() {
        for x in (1..w - 1).rev() {
            let i = y * w + x;
            distance[i] = distance[i]
                .min(distance[i + 1] + 1.)
                .min(distance[i + w] + 1.)
                .min(distance[i + w - 1] + diagonal)
                .min(distance[i + w + 1] + diagonal);
        }
    }
    let image = RgbaImage::from_fn(w as u32, h as u32, |x, y| {
        let d = distance[y as usize * w + x as usize];
        if d == 0. {
            return Rgba([0; 4]);
        }
        let wx = min.x + (x as f32 + 0.5) * step;
        let wy = min.y + (y as f32 + 0.5) * step;
        let depth = ((d - 0.5) * step).max(0.);
        let t = if style.blend_distance > 0. {
            (depth / style.blend_distance).clamp(0., 1.)
        } else {
            1.
        };
        let t = t * t * (3. - 2. * t);
        let noise = (wx * 5.7 + (wy * 3.2).sin()).sin() * (wy * 7.1).cos() * 1.8
            + (wx * 21.3 + wy * 17.9).sin() * 0.7;
        let coast = if style.disable_border {
            0.
        } else {
            let edge = (1. - depth / (0.018 + step * 0.45)).clamp(0., 1.);
            let contour = (1.
                - (depth - 0.16 - (wx * 4. + wy * 3.).sin() * 0.035).abs() / (0.009 + step * 0.3))
                .clamp(0., 1.)
                * ((wx * 8. + wy * 11.).sin() * 3.).clamp(0., 1.)
                * 0.55;
            edge.max(contour)
        };
        Rgba(std::array::from_fn(|i| {
            let base = style.shallow_color[i] as f32 * (1. - t) + style.deep_color[i] as f32 * t;
            if i == 3 {
                base as u8
            } else {
                ((base + noise) * (1. - coast) + [28., 47., 40.][i] * coast).clamp(0., 255.) as u8
            }
        }))
    });
    WaterImage {
        origin: min,
        size: Point::new(w as f32 * step, h as f32 * step),
        image,
    }
}

#[derive(Clone, Default)]
struct Cache(VecDeque<(u64, TextureHandle, Rect, usize)>);

pub fn draw(
    painter: &egui::Painter,
    view: &crate::render::View,
    rings: &[Vec<Point>],
    style: &WaterStyle,
    tint: Color32,
) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for ring in rings {
        ring.len().hash(&mut hasher);
        for p in ring {
            p.x.to_bits().hash(&mut hasher);
            p.y.to_bits().hash(&mut hasher);
        }
    }
    style.deep_color.hash(&mut hasher);
    style.shallow_color.hash(&mut hasher);
    style.blend_distance.to_bits().hash(&mut hasher);
    style.disable_border.hash(&mut hasher);
    let key = hasher.finish();
    let cache_id = egui::Id::new("water-materials");
    let cached = painter.ctx().data_mut(|data| {
        let cache = data.get_temp_mut_or_default::<Cache>(cache_id);
        if let Some(index) = cache.0.iter().position(|entry| entry.0 == key) {
            let entry = cache.0.remove(index).unwrap();
            let result = (entry.1.clone(), entry.2);
            cache.0.push_back(entry);
            return Some(result);
        }
        None
    });
    let (texture, bounds) = cached.unwrap_or_else(|| {
        let water = rasterize(rings, style, 96.);
        let size = [water.image.width() as usize, water.image.height() as usize];
        let bytes = size[0] * size[1] * 4;
        let image = egui::ColorImage::from_rgba_unmultiplied(size, water.image.as_raw());
        let texture =
            painter
                .ctx()
                .load_texture(format!("water:{key}"), image, egui::TextureOptions::LINEAR);
        let rect = Rect::from_min_size(
            egui::pos2(water.origin.x, water.origin.y),
            egui::vec2(water.size.x, water.size.y),
        );
        painter.ctx().data_mut(|data| {
            let cache = data.get_temp_mut_or_default::<Cache>(cache_id);
            while cache.0.iter().map(|e| e.3).sum::<usize>() + bytes > 32 * 1024 * 1024 {
                if cache.0.pop_front().is_none() {
                    break;
                }
            }
            cache.0.push_back((key, texture.clone(), rect, bytes));
        });
        (texture, rect)
    });
    painter.image(
        texture.id(),
        Rect::from_min_max(
            view.screen(Point::new(bounds.min.x, bounds.min.y)),
            view.screen(Point::new(bounds.max.x, bounds.max.y)),
        ),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        tint,
    );
}
