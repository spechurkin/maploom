use crate::{
    assets::{self, Library},
    document::*,
    openings::Openings,
};
use anyhow::{ensure, Context as _, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use egui::{
    epaint::{Mesh, Vertex},
    Color32, Pos2, Rect, Stroke, TextureHandle, Vec2,
};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    path::Path,
};

pub struct View {
    pub origin: Pos2,
    pub scale: f32,
    pub grid: bool,
    pub lights: bool,
}

// Merge brush stamps only while they are adjacent in paint order. Combining
// all matching materials would move later strokes underneath intervening items.
fn terrain_run(items: &[Item], index: usize) -> Option<&[Item]> {
    let same = |a: &Item, b: &Item| {
        matches!(a.kind, ItemKind::Terrain { .. })
            && matches!(b.kind, ItemKind::Terrain { .. })
            && a.texture == b.texture
            && a.border == b.border
            && a.tint == b.tint
            && a.texture_scale == b.texture_scale
    };
    let item = &items[index];
    if index > 0 && same(&items[index - 1], item) {
        return None;
    }
    let count = items[index..]
        .iter()
        .take_while(|next| same(item, next))
        .count();
    Some(&items[index..index + count])
}
#[cfg(test)]
mod strip_tests {
    use super::*;
    #[test]
    fn wall_openings_preserve_texture_phase_and_uncut_corner_joins() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "wall",
            egui::ColorImage::new([32, 8], vec![Color32::WHITE; 256]),
            egui::TextureOptions::LINEAR_REPEAT,
        );
        let points = [Point::new(0., 0.), Point::new(8., 0.), Point::new(8., 6.)];
        let original = strip_mesh(&points, 0.5, 2., &texture, Color32::WHITE, false);
        let mut level = Level::new("Floor");
        level.layers[1].items.push(Item::new(
            "Window",
            ItemKind::Portal {
                bounds: [Point::new(3., 0.), Point::new(5., 0.)],
                closed: true,
                color: [255; 4],
            },
        ));
        let cut = cut_wall_mesh(original.clone(), &points, 0.5, &Openings::new(&level));
        assert_eq!(cut.vertices[2].pos, Pos2::new(3., 0.25));
        assert_eq!(cut.vertices[2].uv.x, 1.5);
        assert_eq!(cut.vertices[4].pos, Pos2::new(5., 0.25));
        assert_eq!(cut.vertices[4].uv.x, 2.5);
        assert_eq!(cut.vertices[6].pos, original.vertices[2].pos);
        assert_eq!(cut.vertices[8].pos, original.vertices[2].pos);
        assert_eq!(cut.vertices[10].uv, original.vertices[4].uv);
    }
    #[test]
    fn terrain_border_has_joined_corners_and_fades_into_the_painted_side() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "border",
            egui::ColorImage::new([32, 8], vec![Color32::WHITE; 256]),
            egui::TextureOptions::LINEAR_REPEAT,
        );
        let points = [
            Point::new(0., 0.),
            Point::new(4., 0.),
            Point::new(4., 4.),
            Point::new(0., 4.),
            Point::new(0., 0.),
        ];
        let mesh = strip_mesh(&points, 1., 3., &texture, Color32::WHITE, true);
        let area: f32 = mesh
            .indices
            .chunks_exact(3)
            .map(|t| {
                let [a, b, c] = [
                    mesh.vertices[t[0] as usize].pos,
                    mesh.vertices[t[1] as usize].pos,
                    mesh.vertices[t[2] as usize].pos,
                ];
                ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() / 2.
            })
            .sum();
        assert!(
            (area - 16.).abs() < 0.001,
            "Corners overlap or leave gaps: {area}"
        );
        assert_eq!(mesh.vertices[0].pos, Pos2::new(-0.5, -0.5));
        assert_eq!(mesh.vertices[2].pos, Pos2::new(0.5, 0.5));
        assert_eq!(mesh.vertices[2].color, Color32::TRANSPARENT);
        assert_eq!(mesh.vertices[0].pos, mesh.vertices[12].pos);
        assert_eq!(mesh.vertices[12].uv.x.fract(), 0.);
        assert!(mesh.vertices.iter().all(|v| v.uv.y > 0. && v.uv.y < 1.));
    }
}
impl View {
    pub fn screen(&self, p: Point) -> Pos2 {
        self.origin + Vec2::new(p.x, p.y) * self.scale
    }
    pub fn world(&self, p: Pos2) -> Point {
        let p = (p - self.origin) / self.scale;
        Point::new(p.x, p.y)
    }
}
pub fn color(c: Color, opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c[0],
        c[1],
        c[2],
        (c[3] as f32 * opacity.clamp(0., 1.)) as u8,
    )
}
pub fn object_corners(position: Point, size: Point, pivot: Point, rotation: f32) -> [Point; 4] {
    let (s, c) = rotation.sin_cos();
    let at = |x: f32, y: f32| {
        let x = (x - pivot.x) * size.x;
        let y = (y - pivot.y) * size.y;
        Point::new(position.x + x * c - y * s, position.y + x * s + y * c)
    };
    [at(0., 0.), at(1., 0.), at(1., 1.), at(0., 1.)]
}
pub fn contains_polygon(points: &[Point], p: Point) -> bool {
    let mut inside = false;
    if points.len() < 3 {
        return false;
    }
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[j];
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside
        }
        j = i;
    }
    inside
}
pub fn segment_distance(p: Point, a: Point, b: Point) -> f32 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = dx * dx + dy * dy;
    if len < 1e-8 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len).clamp(0., 1.);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}
pub fn hit(item: &Item, p: Point, tolerance: f32) -> bool {
    if item.prefab.is_some() {
        return crate::storage::prefabs::parts(item)
            .iter()
            .any(|part| hit(part, p, tolerance));
    }
    if let Some(cave) = &item.cave {
        if crate::geometry::inside(&cave.blasts, p) {
            return false;
        }
    }
    match &item.kind {
        ItemKind::Roof {
            points,
            width,
            roof_type,
            ..
        } => contains_polygon(&crate::roofs::build(points, *width, *roof_type).outline, p),
        ItemKind::Room { rings, .. } | ItemKind::TerrainRegion { rings, .. } => {
            crate::geometry::inside(rings, p)
                || rings.iter().any(|r| {
                (0..r.len()).any(|i| {
                    segment_distance(p, r[i], r[(i + 1) % r.len()]) <= tolerance + 0.15
                })
            })
        }
        ItemKind::Floor { .. } => crate::geometry::inside(&item.floor_rings(), p),
        ItemKind::Path { .. } => crate::paths::item_ribbon(item)
            .chunks(2)
            .collect::<Vec<_>>()
            .windows(2)
            .any(|rows| {
                let quad = [
                    rows[0][0].position,
                    rows[1][0].position,
                    rows[1][1].position,
                    rows[0][1].position,
                ];
                contains_polygon(&quad, p)
                    || (0..4).any(|n| segment_distance(p, quad[n], quad[(n + 1) % 4]) <= tolerance)
            }),
        ItemKind::Wall { points, width, .. } => points
            .windows(2)
            .any(|w| segment_distance(p, w[0], w[1]) <= width / 2. + tolerance),
        ItemKind::Portal { bounds, .. } => {
            segment_distance(p, bounds[0], bounds[1]) <= 0.2 + tolerance
        }
        ItemKind::Object {
            position,
            size,
            pivot,
            rotation,
            ..
        } => contains_polygon(&object_corners(*position, *size, *pivot, *rotation), p),
        ItemKind::Terrain {
            position, radius, ..
        } => p.distance(*position) <= *radius,
        ItemKind::Light { position, .. } => p.distance(*position) <= 0.35 + tolerance,
        ItemKind::Text { .. } => {
            let b = crate::text::bounds(item, 100.);
            p.x >= b.x - tolerance
                && p.x <= b.right + tolerance
                && p.y >= b.y - tolerance
                && p.y <= b.bottom + tolerance
        }
    }
}
pub fn intersects_box(item: &Item, min: Point, max: Point) -> bool {
    let rect = vec![vec![
        min,
        Point::new(max.x, min.y),
        max,
        Point::new(min.x, max.y),
    ]];
    match &item.kind {
        ItemKind::Roof {
            points,
            width,
            roof_type,
            ..
        } => crate::geometry::touches(
            &[crate::roofs::build(points, *width, *roof_type).outline],
            &rect,
        ),
        ItemKind::Object {
            position,
            size,
            pivot,
            rotation,
            ..
        } => crate::geometry::touches(
            &[object_corners(*position, *size, *pivot, *rotation).to_vec()],
            &rect,
        ),
        ItemKind::Room { rings, .. } | ItemKind::TerrainRegion { rings, .. } => {
            crate::geometry::touches(rings, &rect)
        }
        ItemKind::Floor { .. } => crate::geometry::touches(&item.floor_rings(), &rect),
        ItemKind::Terrain {
            position, radius, ..
        }
        | ItemKind::Light {
            position,
            range: radius,
            ..
        } => {
            let near = Point::new(
                position.x.clamp(min.x, max.x),
                position.y.clamp(min.y, max.y),
            );
            position.distance(near) <= *radius
        }
        ItemKind::Text { position, .. } => {
            position.x >= min.x && position.x <= max.x && position.y >= min.y && position.y <= max.y
        }
        ItemKind::Path { .. } => crate::paths::item_ribbon(item)
            .chunks(2)
            .collect::<Vec<_>>()
            .windows(2)
            .any(|rows| {
                crate::geometry::touches(
                    &[vec![
                        rows[0][0].position,
                        rows[1][0].position,
                        rows[1][1].position,
                        rows[0][1].position,
                    ]],
                    &rect,
                )
            }),
        ItemKind::Wall { points, .. } => points
            .windows(2)
            .any(|line| crate::geometry::touches(&[line.to_vec()], &rect)),
        ItemKind::Portal { bounds, .. } => crate::geometry::touches(&[bounds.to_vec()], &rect),
    }
}
// Ear clipping supports concave floor polygons without filling outside the outline.
pub fn triangulate(points: &[Point]) -> Vec<[usize; 3]> {
    if points.len() < 3 {
        return vec![];
    }
    let cross = |a: Point, b: Point, c: Point| {
        (b.x as f64 - a.x as f64) * (c.y as f64 - a.y as f64)
            - (b.y as f64 - a.y as f64) * (c.x as f64 - a.x as f64)
    };
    let area = crate::geometry::area(points);
    let extent = points
        .iter()
        .map(|p| (p.x - points[0].x).abs().max((p.y - points[0].y).abs()))
        .fold(0_f32, f32::max) as f64;
    let epsilon = extent.powi(2) * 1e-12;
    let sign = if area >= 0. { 1. } else { -1. };
    let mut indices: Vec<_> = (0..points.len()).collect();
    if points.first() == points.last() {
        indices.pop();
    }
    let mut result = vec![];
    while indices.len() > 3 {
        let mut ear = None;
        for i in 0..indices.len() {
            let a = indices[(i + indices.len() - 1) % indices.len()];
            let b = indices[i];
            let c = indices[(i + 1) % indices.len()];
            if cross(points[a], points[b], points[c]) * sign <= epsilon {
                continue;
            }
            if indices
                .iter()
                .filter(|&&j| j != a && j != b && j != c)
                .filter(|&&j| {
                    points[j].distance(points[a]) > 1e-6
                        && points[j].distance(points[b]) > 1e-6
                        && points[j].distance(points[c]) > 1e-6
                })
                .any(|&j| {
                    let p = points[j];
                    cross(points[a], points[b], p) * sign >= -epsilon
                        && cross(points[b], points[c], p) * sign >= -epsilon
                        && cross(points[c], points[a], p) * sign >= -epsilon
                })
            {
                continue;
            }
            ear = Some((i, [a, b, c]));
            break;
        }
        if let Some((i, t)) = ear {
            result.push(t);
            indices.remove(i);
        } else {
            break;
        }
    }
    if indices.len() == 3 {
        result.push([indices[0], indices[1], indices[2]]);
    }
    result
}
pub fn draw_scene(
    painter: &egui::Painter,
    project: &Project,
    level: usize,
    textures: &BTreeMap<String, TextureHandle>,
    view: &View,
) {
    let modes = project
        .assets
        .iter()
        .map(|(id, meta)| (id.clone(), meta.object_color_mode()))
        .collect();
    draw_scene_with_modes(painter, project, level, textures, &modes, view);
}
pub fn draw_scene_with_modes(
    painter: &egui::Painter,
    project: &Project,
    level: usize,
    textures: &BTreeMap<String, TextureHandle>,
    modes: &BTreeMap<String, ObjectColorMode>,
    view: &View,
) {
    draw_scene_impl(
        painter,
        project,
        level,
        &SceneTextures {
            textures,
            modes,
            sizes: None,
            shader_masks: false,
            blockers: None,
        },
        view,
        None,
    );
}
/// Shape spans keep picking attached to the very same geometry as painting.
pub(crate) struct ItemShapes {
    pub range: std::ops::Range<usize>,
    pub item: String,
}
struct ShapeOwner<'a> {
    painter: &'a egui::Painter,
    spans: Option<&'a std::cell::RefCell<Vec<ItemShapes>>>,
    start: usize,
    item: &'a str,
}
impl<'a> ShapeOwner<'a> {
    fn new(
        painter: &'a egui::Painter,
        spans: Option<&'a std::cell::RefCell<Vec<ItemShapes>>>,
        item: &'a str,
    ) -> Self {
        let start = if spans.is_some() {
            painter
                .ctx()
                .graphics_mut(|g| g.entry(painter.layer_id()).next_idx().0)
        } else {
            0
        };
        Self {
            painter,
            spans,
            start,
            item,
        }
    }
}
impl Drop for ShapeOwner<'_> {
    fn drop(&mut self) {
        if let Some(spans) = self.spans {
            let end = self
                .painter
                .ctx()
                .graphics_mut(|g| g.entry(self.painter.layer_id()).next_idx().0);
            if end > self.start {
                spans.borrow_mut().push(ItemShapes {
                    range: self.start..end,
                    item: self.item.to_owned(),
                });
            }
        }
    }
}
pub(crate) struct SceneTextures<'a> {
    pub textures: &'a BTreeMap<String, TextureHandle>,
    pub modes: &'a BTreeMap<String, ObjectColorMode>,
    pub sizes: Option<&'a BTreeMap<String, Vec2>>,
    pub shader_masks: bool,
    pub blockers: Option<&'a crate::light::Blockers>,
}
pub(crate) fn draw_scene_gpu(
    painter: &egui::Painter,
    project: &Project,
    level: usize,
    resources: &SceneTextures<'_>,
    view: &View,
) -> Vec<ItemShapes> {
    let spans = std::cell::RefCell::new(Vec::new());
    draw_scene_impl(painter, project, level, resources, view, Some(&spans));
    spans.into_inner()
}
fn draw_scene_impl(
    painter: &egui::Painter,
    project: &Project,
    level: usize,
    resources: &SceneTextures<'_>,
    view: &View,
    spans: Option<&std::cell::RefCell<Vec<ItemShapes>>>,
) {
    let expanded = crate::storage::prefabs::expanded(project);
    let project = expanded.as_ref();
    let SceneTextures {
        textures,
        modes,
        sizes,
        shader_masks,
        blockers,
    } = resources;
    let fallback_blockers;
    let blockers = if let Some(blockers) = blockers {
        *blockers
    } else {
        fallback_blockers = crate::light::Blockers::new(project, level, &Library::new());
        &fallback_blockers
    };
    let texture_size = |id: &str, texture: &TextureHandle| {
        sizes
            .and_then(|s| s.get(id).copied())
            .unwrap_or_else(|| texture.size_vec2())
    };
    let map_rect = Rect::from_min_size(
        view.origin,
        Vec2::new(project.width as f32, project.height as f32) * view.scale,
    );
    let painter = painter.with_clip_rect(painter.clip_rect().intersect(map_rect));
    let start = painter
        .ctx()
        .graphics_mut(|g| g.entry(painter.layer_id()).next_idx());
    crate::light::begin_scene(painter.ctx());
    painter.rect_filled(map_rect, 0., color(project.background, 1.));
    if let Some(texture) = project
        .background_asset
        .as_ref()
        .and_then(|id| textures.get(id))
    {
        let mut mesh = Mesh::with_texture(texture.id());
        mesh.add_rect_with_uv(
            map_rect,
            Rect::from_min_max(
                Pos2::ZERO,
                Pos2::new(
                    project.width as f32
                        / (texture_size(
                        project.background_asset.as_deref().unwrap_or_default(),
                        texture,
                    )
                        .x / 256.),
                    project.height as f32
                        / (texture_size(
                        project.background_asset.as_deref().unwrap_or_default(),
                        texture,
                    )
                        .y / 256.),
                ),
            ),
            Color32::WHITE,
        );
        painter.add(egui::Shape::mesh(mesh));
    }
    let openings = Openings::new(&project.levels[level]);
    let cave_rooms = crate::caves::rooms(&project.levels[level]);
    let passes = landscape_passes(&project.levels[level]);
    for pass in 0..passes {
        for layer in project.levels[level].layers.iter().filter(|l| l.visible) {
            let opacity = layer.opacity;
            for (index, item) in layer.items.iter().enumerate() {
                if passes > 1 && landscape_pass(item) != pass {
                    continue;
                }
                let _owner = ShapeOwner::new(&painter, spans, &item.id);
                let openings = openings.for_wall(&item.id);
                let texture = item.texture.as_ref().and_then(|id| textures.get(id));
                match &item.kind {
                    ItemKind::Roof {
                        points,
                        width,
                        roof_type,
                        ridge,
                        edge,
                        hip,
                        shade,
                        sun_direction,
                        contrast,
                    } => {
                        let roof = crate::roofs::build(points, *width, *roof_type);
                        if let Some(texture) = texture {
                            for facet in &roof.facets {
                                let mut mesh = Mesh::with_texture(texture.id());
                                let brightness =
                                    facet.brightness(*shade, *sun_direction, *contrast);
                                let tint = multiply_colors(
                                    item.tint,
                                    [
                                        (brightness * 255.) as u8,
                                        (brightness * 255.) as u8,
                                        (brightness * 255.) as u8,
                                        255,
                                    ],
                                );
                                for p in &facet.points {
                                    let uv = facet.uv(*p, item.texture_scale);
                                    mesh.vertices.push(Vertex {
                                        pos: view.screen(*p),
                                        uv: Pos2::new(uv.x, uv.y),
                                        color: color(tint, opacity),
                                    });
                                }
                                for t in triangulate(&facet.points) {
                                    mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
                                }
                                painter.add(egui::Shape::mesh(mesh));
                            }
                        }
                        for (id, lines) in [
                            (
                                edge,
                                vec![roof
                                    .outline
                                    .iter()
                                    .copied()
                                    .chain(roof.outline.first().copied())
                                    .collect::<Vec<_>>()],
                            ),
                            (hip, roof.hips.iter().map(|line| line.to_vec()).collect()),
                            (ridge, vec![roof.ridge]),
                        ] {
                            if let Some((id, texture)) =
                                id.as_ref().and_then(|id| textures.get(id).map(|t| (id, t)))
                            {
                                let size = texture_size(id, texture) / 256.;
                                let tile_size = item
                                    .texture
                                    .as_ref()
                                    .and_then(|id| {
                                        textures.get(id).map(|t| texture_size(id, t).x / 256.)
                                    })
                                    .unwrap_or(4.);
                                let factor = item.texture_scale / tile_size.max(0.01);
                                for line in lines {
                                    draw_textured_line(
                                        &painter,
                                        view,
                                        &line,
                                        LineStyle {
                                            width: (size.y * factor).max(0.015),
                                            scale: size.x * factor,
                                            texture,
                                            tint: color(item.tint, opacity),
                                        },
                                        None,
                                    );
                                }
                            }
                        }
                    }
                    ItemKind::Room {
                        rings,
                        floor,
                        wall,
                        width,
                    } => {
                        if let Some(texture) = floor.as_ref().and_then(|id| textures.get(id)) {
                            draw_rings(
                                &painter,
                                view,
                                rings,
                                item.texture_scale,
                                texture,
                                color(multiply_colors(item.tint, item.floor_tint), opacity),
                            );
                        }
                        if let Some(texture) = wall.as_ref().and_then(|id| textures.get(id)) {
                            for (_, ring) in rings
                                .iter()
                                .enumerate()
                                .filter(|(n, _)| !item.wall_exclusions.contains(n))
                            {
                                let mut closed = ring.clone();
                                closed.push(ring[0]);
                                draw_textured_line(
                                    &painter,
                                    view,
                                    &closed,
                                    LineStyle {
                                        width: *width,
                                        scale: (texture_size(
                                            wall.as_deref().unwrap_or_default(),
                                            texture,
                                        )
                                            .x / 256.)
                                            .max(0.1),
                                        texture,
                                        tint: color(
                                            multiply_colors(item.tint, item.wall_tint),
                                            opacity,
                                        ),
                                    },
                                    Some(&openings),
                                );
                            }
                        }
                    }
                    ItemKind::Floor { .. } => {
                        let rings = item.floor_rings();
                        if let Some(texture) = texture {
                            draw_rings(
                                &painter,
                                view,
                                &rings,
                                item.texture_scale,
                                texture,
                                color(item.tint, opacity),
                            );
                        }
                        for ring in rings.iter().filter(|_| item.floor_outline != Some(false)) {
                            painter.add(egui::Shape::closed_line(
                                ring.iter().map(|p| view.screen(*p)).collect(),
                                Stroke::new(
                                    1.,
                                    if texture.is_some() {
                                        color([39, 44, 38, 90], opacity)
                                    } else {
                                        Color32::from_rgb(217, 109, 125)
                                    },
                                ),
                            ));
                        }
                    }
                    ItemKind::Path { points, .. } => {
                        if let Some(texture) = texture {
                            let vertices = crate::paths::item_ribbon(item);
                            let mut mesh = Mesh::with_texture(texture.id());
                            let inset = 0.5 / texture.size_vec2().y.max(1.);
                            for v in vertices {
                                mesh.vertices.push(Vertex {
                                    pos: view.screen(v.position),
                                    uv: Pos2::new(v.uv.x, inset + v.uv.y * (1. - 2. * inset)),
                                    color: color(item.tint, opacity * v.alpha),
                                });
                            }
                            for n in (2..mesh.vertices.len()).step_by(2) {
                                let n = n as u32;
                                mesh.indices.extend_from_slice(&[
                                    n - 2,
                                    n,
                                    n + 1,
                                    n - 2,
                                    n + 1,
                                    n - 1,
                                ]);
                            }
                            painter.add(egui::Shape::mesh(mesh));
                        } else {
                            let ps = crate::paths::sample(
                                points,
                                item.path_style.as_ref().map_or(0., |s| s.smoothness),
                            );
                            painter.add(egui::Shape::line(
                                ps.into_iter().map(|p| view.screen(p)).collect(),
                                Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                            ));
                        }
                    }
                    ItemKind::Wall {
                        points,
                        width,
                        color: _c,
                        ..
                    } => {
                        if let Some(texture) = texture {
                            draw_textured_line(
                                &painter,
                                view,
                                points,
                                LineStyle {
                                    width: *width,
                                    scale: item.texture_scale,
                                    texture,
                                    tint: color(item.tint, opacity),
                                },
                                matches!(item.kind, ItemKind::Wall { .. }).then_some(&openings),
                            );
                            continue;
                        }
                        if matches!(item.kind, ItemKind::Path { .. }) {
                            painter.add(egui::Shape::line(
                                points.iter().map(|p| view.screen(*p)).collect(),
                                Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                            ));
                            continue;
                        }
                        for line in points.windows(2) {
                            let line = [line[0], line[1]];
                            for (lo, hi) in openings.spans(line) {
                                painter.line_segment(
                                    [
                                        view.screen(crate::openings::at(line, lo)),
                                        view.screen(crate::openings::at(line, hi)),
                                    ],
                                    Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                                );
                            }
                        }
                    }
                    ItemKind::TerrainRegion { rings, .. } => {
                        if let Some(cave) = &item.cave {
                            if let Some(texture) = texture {
                                let floor = item
                                    .border
                                    .as_ref()
                                    .and_then(|id| {
                                        textures.get(id).map(|t| {
                                            crate::caves::render_floor(
                                                item,
                                                &cave_rooms,
                                                texture_size(id, t).y / 256.,
                                            )
                                        })
                                    })
                                    .unwrap_or_else(|| crate::caves::floor(item, &cave_rooms));
                                draw_rings(
                                    &painter,
                                    view,
                                    &floor,
                                    item.texture_scale,
                                    texture,
                                    color(
                                        multiply_colors(
                                            item.tint,
                                            if cave.colorable {
                                                cave.ground_color
                                            } else {
                                                [255; 4]
                                            },
                                        ),
                                        opacity,
                                    ),
                                );
                            }
                            if let Some((id, border)) = item
                                .border
                                .as_ref()
                                .and_then(|id| textures.get(id).map(|t| (id, t)))
                            {
                                let size = texture_size(id, border) / 256.;
                                let masks = crate::geometry::union_rooms(&[
                                    cave.blasts.clone(),
                                    cave_rooms.clone(),
                                ])
                                    .unwrap_or_default();
                                for ring in rings {
                                    let ribbon = crate::caves::wall_ribbon(ring, size.y, size.x);
                                    let tint = color(
                                        multiply_colors(
                                            item.tint,
                                            if cave.colorable {
                                                cave.wall_color
                                            } else {
                                                [255; 4]
                                            },
                                        ),
                                        opacity,
                                    );
                                    let mut mesh = Mesh::with_texture(border.id());
                                    let inset = 0.5 / border.size_vec2().y.max(1.);
                                    mesh.vertices = ribbon
                                        .iter()
                                        .map(|v| Vertex {
                                            pos: Pos2::new(v.position.x, v.position.y),
                                            uv: Pos2::new(
                                                v.uv.x,
                                                inset + v.uv.y * (1. - 2. * inset),
                                            ),
                                            color: tint,
                                        })
                                        .collect();
                                    for n in (2..ribbon.len() as u32).step_by(2) {
                                        mesh.indices.extend_from_slice(&[
                                            n - 2,
                                            n,
                                            n + 1,
                                            n - 2,
                                            n + 1,
                                            n - 1,
                                        ]);
                                    }
                                    let mut mesh = clip_cave_mesh(mesh, &masks);
                                    for v in &mut mesh.vertices {
                                        v.pos = view.screen(Point::new(v.pos.x, v.pos.y));
                                    }
                                    painter.add(egui::Shape::mesh(mesh));
                                }
                            }
                            continue;
                        }
                        if let Some(water) = &item.water {
                            crate::water::draw(
                                &painter,
                                view,
                                rings,
                                water,
                                color(item.tint, opacity),
                            );
                            continue;
                        }
                        if let Some(texture) = texture {
                            draw_rings(
                                &painter,
                                view,
                                rings,
                                item.texture_scale,
                                texture,
                                color(item.tint, opacity),
                            );
                            if let Some(border) =
                                item.border.as_ref().and_then(|id| textures.get(id))
                            {
                                let width = (border.size_vec2().y / 256.).clamp(0.08, 2.);
                                let scale = (border.size_vec2().x / 256.).max(0.1);
                                for ring in rings {
                                    let mut closed = ring.clone();
                                    closed.push(ring[0]);
                                    let mut mesh = strip_mesh(
                                        &closed,
                                        width,
                                        scale,
                                        border,
                                        color(item.tint, opacity),
                                        true,
                                    );
                                    for vertex in &mut mesh.vertices {
                                        vertex.pos =
                                            view.screen(Point::new(vertex.pos.x, vertex.pos.y));
                                    }
                                    painter.add(egui::Shape::mesh(mesh));
                                }
                            }
                        } else {
                            for ring in rings {
                                painter.add(egui::Shape::closed_line(
                                    ring.iter().map(|p| view.screen(*p)).collect(),
                                    Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                                ));
                            }
                        }
                    }
                    ItemKind::Terrain {
                        position, radius, ..
                    } => {
                        if let Some(texture) = texture {
                            let Some(run) = terrain_run(&layer.items, index) else {
                                continue;
                            };
                            let stamps: Vec<_> = run
                                .iter()
                                .filter_map(|i| {
                                    if let ItemKind::Terrain {
                                        position, radius, ..
                                    } = i.kind
                                    {
                                        Some((position, radius))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            use std::hash::{Hash, Hasher};
                            let mut hasher = std::collections::hash_map::DefaultHasher::new();
                            for (p, r) in &stamps {
                                p.x.to_bits().hash(&mut hasher);
                                p.y.to_bits().hash(&mut hasher);
                                r.to_bits().hash(&mut hasher);
                            }
                            let fingerprint = hasher.finish();
                            let id = egui::Id::new((
                                &project.id,
                                &layer.id,
                                &item.id,
                                item.texture_scale.to_bits(),
                            ));
                            let cached =
                                painter.ctx().data_mut(|d| d.get_temp::<TerrainCached>(id));
                            let cached = if let Some(cached) =
                                cached.filter(|c| c.fingerprint == fingerprint)
                            {
                                cached
                            } else {
                                let rings =
                                    std::sync::Arc::new(crate::geometry::terrain_contours(&stamps));
                                let fill =
                                    std::sync::Arc::new(ring_meshes(&rings, item.texture_scale));
                                let cached = TerrainCached {
                                    fingerprint,
                                    rings,
                                    fill,
                                };
                                painter
                                    .ctx()
                                    .data_mut(|d| d.insert_temp(id, cached.clone()));
                                cached
                            };
                            let rings = &cached.rings;
                            draw_meshes(
                                &painter,
                                view,
                                &cached.fill,
                                texture,
                                color(item.tint, opacity),
                            );
                            if let Some(border) =
                                item.border.as_ref().and_then(|id| textures.get(id))
                            {
                                let width = (border.size_vec2().y / 256.).clamp(0.08, 2.);
                                let scale = (border.size_vec2().x / 256.).max(0.1);
                                for ring in rings.iter() {
                                    let mut closed = ring.clone();
                                    closed.push(ring[0]);
                                    let mut mesh = strip_mesh(
                                        &closed,
                                        width,
                                        scale,
                                        border,
                                        color(item.tint, opacity),
                                        true,
                                    );
                                    for vertex in &mut mesh.vertices {
                                        vertex.pos =
                                            view.screen(Point::new(vertex.pos.x, vertex.pos.y));
                                    }
                                    painter.add(egui::Shape::mesh(mesh));
                                }
                            }
                        } else {
                            painter.circle_stroke(
                                view.screen(*position),
                                radius * view.scale,
                                Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                            );
                        }
                    }
                    ItemKind::Object {
                        asset,
                        position,
                        size,
                        pivot,
                        rotation,
                        flip_x,
                        flip_y,
                    } => {
                        let corners = object_corners(*position, *size, *pivot, *rotation);
                        let mode = modes.get(asset).copied().unwrap_or_default();
                        let key = if mode == ObjectColorMode::RedMask {
                            assets::color_texture_key(asset, item.tint)
                        } else {
                            asset.clone()
                        };
                        let texture = textures.get(&key).or_else(|| {
                            if mode == ObjectColorMode::RedMask {
                                textures.get(&assets::color_fallback_key(asset))
                            } else {
                                None
                            }
                        });
                        let object_color = if mode == ObjectColorMode::Tint
                            || (*shader_masks && mode == ObjectColorMode::RedMask)
                        {
                            color(item.tint, opacity)
                        } else {
                            color([255; 4], opacity)
                        };
                        if let Some(texture) = texture {
                            let mut mesh = Mesh::with_texture(texture.id());
                            for (i, p) in corners.iter().enumerate() {
                                let (x, y) = match i {
                                    0 => (0., 0.),
                                    1 => (1., 0.),
                                    2 => (1., 1.),
                                    _ => (0., 1.),
                                };
                                mesh.vertices.push(Vertex {
                                    pos: view.screen(*p),
                                    uv: Pos2::new(
                                        if *flip_x { 1. - x } else { x },
                                        if *flip_y { 1. - y } else { y },
                                    ),
                                    color: object_color,
                                });
                            }
                            mesh.indices = vec![0, 1, 2, 0, 2, 3];
                            painter.add(egui::Shape::mesh(mesh));
                        } else {
                            painter.add(egui::Shape::closed_line(
                                corners.iter().map(|p| view.screen(*p)).collect(),
                                Stroke::new(1.5, Color32::from_rgb(217, 109, 125)),
                            ));
                            painter.line_segment(
                                [view.screen(corners[0]), view.screen(corners[2])],
                                Stroke::new(1., Color32::from_rgb(217, 109, 125)),
                            );
                        }
                    }
                    ItemKind::Portal {
                        bounds,
                        closed,
                        color: c,
                    } => {
                        if let Some(texture) = texture {
                            let position = Point::new(
                                (bounds[0].x + bounds[1].x) / 2.,
                                (bounds[0].y + bounds[1].y) / 2.,
                            );
                            let length = bounds[0].distance(bounds[1]);
                            let size = Point::new(
                                length,
                                length
                                    * texture_size(
                                    item.texture.as_deref().unwrap_or_default(),
                                    texture,
                                )
                                    .y
                                    / texture_size(
                                    item.texture.as_deref().unwrap_or_default(),
                                    texture,
                                )
                                    .x,
                            );
                            let rotation =
                                (bounds[1].y - bounds[0].y).atan2(bounds[1].x - bounds[0].x);
                            draw_quad(
                                &painter,
                                view,
                                &object_corners(position, size, Point::new(0.5, 0.5), rotation),
                                texture,
                                color(item.tint, opacity),
                            );
                            continue;
                        }
                        painter.line_segment(
                            [view.screen(bounds[0]), view.screen(bounds[1])],
                            Stroke::new(
                                0.13 * view.scale,
                                color(if *closed { *c } else { [102, 193, 207, 255] }, opacity),
                            ),
                        );
                    }
                    ItemKind::Light { .. } => {}
                    ItemKind::Text { position, .. } => {
                        crate::text::paint(
                            &painter,
                            item,
                            project.pixels_per_grid as f32,
                            view.screen(*position),
                            view.scale,
                            opacity,
                        );
                    }
                }
            }
        }
    }
    // Apply ambient color to the map before drawing sources. GPU masks are
    // multiplied after recoloring in the scene builder instead.
    if view.lights && !shader_masks && project.ambient_light[..3] != [255; 3] {
        let ambient = project.ambient_light;
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            let end = list.next_idx();
            for index in start.0..end.0 {
                list.mutate_shape(egui::layers::ShapeIdx(index), |shape| {
                    egui::epaint::shape_transform::adjust_colors(&mut shape.shape, move |c| {
                        if *c == Color32::PLACEHOLDER {
                            return;
                        }
                        let [r, g, b, a] = c.to_array();
                        let multiply =
                            |v: u8, tint: u8| ((v as u32 * tint as u32 + 127) / 255) as u8;
                        *c = Color32::from_rgba_premultiplied(
                            multiply(r, ambient[0]),
                            multiply(g, ambient[1]),
                            multiply(b, ambient[2]),
                            a,
                        );
                    });
                });
            }
        });
    }
    if view.lights {
        for layer in project.levels[level].layers.iter().filter(|l| l.visible) {
            for item in &layer.items {
                if let ItemKind::Light {
                    position,
                    range,
                    intensity,
                    smoothing,
                    color: c,
                    shadows,
                } = &item.kind
                {
                    let _owner = ShapeOwner::new(&painter, spans, &item.id);
                    let visibility = if *shadows {
                        blockers.visibility(*position, *range)
                    } else {
                        None
                    };
                    crate::light::draw(
                        &painter,
                        view,
                        crate::light::Stamp {
                            position: *position,
                            range: *range,
                            smoothing: *smoothing,
                            intensity: *intensity,
                            tint: color(*c, layer.opacity),
                        },
                        visibility.as_deref(),
                    );
                }
            }
        }
    }
    if view.grid && view.scale >= 8. {
        let stroke = Stroke::new(0.65, Color32::from_black_alpha(35));
        for x in 0..=project.width {
            let p = view.screen(Point::new(x as f32, 0.));
            painter.line_segment(
                [p, p + Vec2::new(0., project.height as f32 * view.scale)],
                stroke,
            );
        }
        for y in 0..=project.height {
            let p = view.screen(Point::new(0., y as f32));
            painter.line_segment(
                [p, p + Vec2::new(project.width as f32 * view.scale, 0.)],
                stroke,
            );
        }
    }
}
#[derive(Clone)]
struct TerrainCached {
    fingerprint: u64,
    rings: std::sync::Arc<Vec<Vec<Point>>>,
    fill: std::sync::Arc<Vec<Mesh>>,
}
#[derive(Default, Clone)]
struct RingGeometryCache {
    entries: BTreeMap<u64, CachedRingGeometry>,
}
type CachedRingGeometry = (Vec<Vec<Point>>, std::sync::Arc<Vec<Mesh>>);
fn draw_rings(
    painter: &egui::Painter,
    view: &View,
    rings: &[Vec<Point>],
    scale: f32,
    texture: &TextureHandle,
    tint: Color32,
) {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for ring in rings {
        ring.len().hash(&mut hasher);
        for p in ring {
            p.x.to_bits().hash(&mut hasher);
            p.y.to_bits().hash(&mut hasher);
        }
    }
    let key = hasher.finish();
    let cache_id = egui::Id::new("maploom-room-geometry");
    let cached = painter.ctx().data_mut(|d| {
        d.get_temp_mut_or_default::<RingGeometryCache>(cache_id)
            .entries
            .get(&key)
            .filter(|(r, _)| r.as_slice() == rings)
            .map(|(_, m)| m.clone())
    });
    let meshes = cached.unwrap_or_else(|| {
        let meshes = std::sync::Arc::new(ring_meshes(rings, 1.));
        painter.ctx().data_mut(|d| {
            let cache = d.get_temp_mut_or_default::<RingGeometryCache>(cache_id);
            if cache.entries.len() >= 128 {
                cache.entries.clear();
            }
            cache.entries.insert(key, (rings.to_vec(), meshes.clone()));
        });
        meshes
    });
    for mesh in meshes.iter() {
        let mut mesh = mesh.clone();
        mesh.texture_id = texture.id();
        for v in &mut mesh.vertices {
            v.uv = Pos2::new(v.pos.x / scale, v.pos.y / scale);
            v.pos = view.screen(Point::new(v.pos.x, v.pos.y));
            v.color = tint;
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}
fn ring_meshes(rings: &[Vec<Point>], scale: f32) -> Vec<Mesh> {
    crate::geometry::fill_polygons(rings)
        .into_iter()
        .map(|points| {
            let mut mesh = Mesh::default();
            for p in &points {
                mesh.vertices.push(Vertex {
                    pos: Pos2::new(p.x, p.y),
                    uv: Pos2::new(p.x / scale, p.y / scale),
                    color: Color32::WHITE,
                });
            }
            for t in triangulate(&points) {
                mesh.add_triangle(t[0] as u32, t[1] as u32, t[2] as u32);
            }
            mesh
        })
        .collect()
}
fn draw_meshes(
    painter: &egui::Painter,
    view: &View,
    meshes: &[Mesh],
    texture: &TextureHandle,
    tint: Color32,
) {
    for mesh in meshes {
        let mut mesh = mesh.clone();
        mesh.texture_id = texture.id();
        for vertex in &mut mesh.vertices {
            vertex.pos = view.screen(Point::new(vertex.pos.x, vertex.pos.y));
            vertex.color = tint;
        }
        painter.add(egui::Shape::mesh(mesh));
    }
}
fn draw_quad(
    painter: &egui::Painter,
    view: &View,
    corners: &[Point; 4],
    texture: &TextureHandle,
    tint: Color32,
) {
    let mut mesh = Mesh::with_texture(texture.id());
    for (p, uv) in corners.iter().zip([
        Pos2::new(0., 0.),
        Pos2::new(1., 0.),
        Pos2::new(1., 1.),
        Pos2::new(0., 1.),
    ]) {
        mesh.vertices.push(Vertex {
            pos: view.screen(*p),
            uv,
            color: tint,
        });
    }
    mesh.indices = vec![0, 1, 2, 0, 2, 3];
    painter.add(egui::Shape::mesh(mesh));
}
// Clip the full rock strip, including its thickness, so a blast never leaves
// texture fragments extending into the opening. UVs remain on the original strip.
fn clip_cave_mesh(original: Mesh, masks: &[Vec<Point>]) -> Mesh {
    if masks.is_empty() {
        return original;
    }
    let mut mesh = Mesh::with_texture(original.texture_id);
    for indices in original.indices.chunks_exact(3) {
        let [a, b, c] = [
            original.vertices[indices[0] as usize],
            original.vertices[indices[1] as usize],
            original.vertices[indices[2] as usize],
        ];
        let triangle = vec![
            Point::new(a.pos.x, a.pos.y),
            Point::new(b.pos.x, b.pos.y),
            Point::new(c.pos.x, c.pos.y),
        ];
        let cross = |u: Vec2, v: Vec2| u.x * v.y - u.y * v.x;
        let denominator = cross(b.pos - a.pos, c.pos - a.pos);
        if denominator.abs() < 1e-10 {
            continue;
        }
        for polygon in crate::geometry::fill_polygons(&crate::caves::difference(&[triangle], masks))
        {
            let base = mesh.vertices.len() as u32;
            for p in &polygon {
                let pos = Pos2::new(p.x, p.y);
                let u = cross(pos - a.pos, c.pos - a.pos) / denominator;
                let v = cross(b.pos - a.pos, pos - a.pos) / denominator;
                mesh.vertices.push(Vertex {
                    pos,
                    uv: a.uv + (b.uv - a.uv) * u + (c.uv - a.uv) * v,
                    color: a.color,
                });
            }
            for t in triangulate(&polygon) {
                mesh.add_triangle(base + t[0] as u32, base + t[1] as u32, base + t[2] as u32);
            }
        }
    }
    mesh
}

struct LineStyle<'a> {
    width: f32,
    scale: f32,
    texture: &'a TextureHandle,
    tint: Color32,
}

fn draw_textured_line(
    painter: &egui::Painter,
    view: &View,
    points: &[Point],
    style: LineStyle<'_>,
    openings: Option<&Openings>,
) {
    let LineStyle {
        width,
        scale,
        texture,
        tint,
    } = style;
    let mut mesh = strip_mesh(points, width, scale, texture, tint, false);
    if let Some(openings) = openings {
        mesh = cut_wall_mesh(mesh, points, width, openings);
    }
    for vertex in &mut mesh.vertices {
        vertex.pos = view.screen(Point::new(vertex.pos.x, vertex.pos.y));
    }
    painter.add(egui::Shape::mesh(mesh));
}

// Keep corner joins and texture coordinates from the unbroken strip. New cut
// endpoints are perpendicular to the wall, without restarting its texture.
fn cut_wall_mesh(original: Mesh, points: &[Point], width: f32, openings: &Openings) -> Mesh {
    let mut points = points.to_vec();
    points.dedup_by(|a, b| a.distance(*b) < 0.00001);
    let spans: Vec<_> = points
        .windows(2)
        .map(|p| openings.spans([p[0], p[1]]))
        .collect();
    if spans.iter().all(|s| s == &[(0., 1.)]) {
        return original;
    }
    let mut mesh = Mesh::with_texture(original.texture_id);
    for (i, pair) in points.windows(2).enumerate() {
        let line = [pair[0], pair[1]];
        let length = line[0].distance(line[1]);
        let normal = Vec2::new(-(line[1].y - line[0].y), line[1].x - line[0].x) / length;
        for &(lo, hi) in &spans[i] {
            let base = mesh.vertices.len() as u32;
            for t in [lo, hi] {
                for row in 0..2 {
                    let a = original.vertices[i * 2 + row];
                    let b = original.vertices[(i + 1) * 2 + row];
                    let center = crate::openings::at(line, t);
                    let pos = if t == 0. {
                        a.pos
                    } else if t == 1. {
                        b.pos
                    } else {
                        Pos2::new(center.x, center.y)
                            + normal * (width * if row == 0 { 0.5 } else { -0.5 })
                    };
                    mesh.vertices.push(Vertex {
                        pos,
                        uv: a.uv + (b.uv - a.uv) * t,
                        color: a.color,
                    });
                }
            }
            mesh.indices
                .extend_from_slice(&[base, base + 2, base + 3, base, base + 3, base + 1]);
        }
    }
    mesh
}
fn strip_mesh(
    points: &[Point],
    width: f32,
    scale: f32,
    texture: &TextureHandle,
    tint: Color32,
    terrain: bool,
) -> Mesh {
    let mut points = points.to_vec();
    points.dedup_by(|a, b| a.distance(*b) < 0.00001);
    let mut mesh = Mesh::with_texture(texture.id());
    if points.len() < 2 {
        return mesh;
    }
    let closed = points.len() > 3 && points[0].distance(*points.last().unwrap()) < 0.00001;
    let count = if closed {
        points.len() - 1
    } else {
        points.len()
    };
    let perimeter: f32 = points.windows(2).map(|w| w[0].distance(w[1])).sum();
    let scale = if terrain && closed {
        perimeter / (perimeter / scale.max(0.1)).round().max(1.)
    } else {
        scale.max(0.01)
    };
    let normal = |a: Point, b: Point| {
        let length = a.distance(b).max(0.00001);
        Vec2::new(-(b.y - a.y) / length, (b.x - a.x) / length)
    };
    let inset = 0.5 / texture.size_vec2().y.max(1.);
    let rows = if terrain { 3 } else { 2 };
    let mut distance = 0.;
    for i in 0..points.len() {
        if i > 0 {
            distance += points[i - 1].distance(points[i]);
        }
        let index = i % count;
        let a = points[index];
        let incoming = if index > 0 {
            normal(points[index - 1], a)
        } else if closed {
            normal(points[count - 1], a)
        } else {
            normal(a, points[1])
        };
        let outgoing = if index + 1 < count {
            normal(a, points[index + 1])
        } else if closed {
            normal(a, points[0])
        } else {
            incoming
        };
        let sum = incoming + outgoing;
        let averaged = if sum.length_sq() > 0.00001 {
            sum.normalized()
        } else {
            outgoing
        };
        // Joined strips share their vertices. The capped miter prevents spikes at tight turns.
        let miter = averaged / averaged.dot(outgoing).max(0.5);
        for row in 0..rows {
            let (offset, v, color) = if terrain {
                match row {
                    0 => (-0.5, inset, tint),
                    1 => (0., 0.5, tint),
                    _ => (0.5, 1. - inset, Color32::TRANSPARENT),
                }
            } else if row == 0 {
                (0.5, inset, tint)
            } else {
                (-0.5, 1. - inset, tint)
            };
            let pos = Pos2::new(a.x, a.y) + miter * (width * offset);
            mesh.vertices.push(Vertex {
                pos,
                uv: Pos2::new(distance / scale, v),
                color,
            });
        }
        if i > 0 {
            for row in 0..rows - 1 {
                let a = ((i - 1) * rows + row) as u32;
                let b = (i * rows + row) as u32;
                mesh.indices
                    .extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
            }
        }
    }
    mesh
}
#[derive(Clone, Copy)]
pub struct ExportOptions {
    pub level: usize,
    pub pixels_per_grid: u32,
    pub grid: bool,
    pub lights: bool,
    pub jpeg_quality: u8,
}
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub info: wgpu::AdapterInfo,
}
pub fn gpu_backends() -> wgpu::Backends {
    wgpu::Backends::from_env().unwrap_or(
        wgpu::Backends::VULKAN
            | wgpu::Backends::METAL
            | wgpu::Backends::DX12
            | wgpu::Backends::BROWSER_WEBGPU,
    )
}
pub fn create_gpu() -> Result<Gpu> {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: gpu_backends(),
            ..Default::default()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
            })
            .await
            .context("No compatible GPU adapter")?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Maploom export"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await?;
        Ok(Gpu {
            device,
            queue,
            info,
        })
    })
}
pub fn rasterize(
    project: &Project,
    library: &Library,
    options: ExportOptions,
) -> Result<(image::RgbaImage, String)> {
    project.validate()?;
    ensure!(options.level < project.levels.len(), "Invalid export level");
    rasterize_surface(
        &project.export_surface(options.level),
        library,
        ExportOptions {
            level: 0,
            ..options
        },
    )
}

fn rasterize_surface(
    project: &Project,
    library: &Library,
    options: ExportOptions,
) -> Result<(image::RgbaImage, String)> {
    let mut resolved = crate::storage::prefabs::expanded(project).into_owned();
    crate::formats::resolve_dd_assets(&mut resolved, library)?;
    let project = &resolved;
    project.validate()?;
    ensure!(options.level < project.levels.len(), "Invalid export level");
    assets::require_assets(project, options.level, library)?;
    ensure!(
        (16..=512).contains(&options.pixels_per_grid),
        "Pixels per grid must be 16–512"
    );
    let width = project.width * options.pixels_per_grid;
    let height = project.height * options.pixels_per_grid;
    ensure!(
        width as u64 * height as u64 <= 64 * 1024 * 1024,
        "Export exceeds 64 megapixels; lower the resolution"
    );
    let gpu = create_gpu()?;
    let limit = gpu.device.limits().max_texture_dimension_2d;
    ensure!(
        width <= limit && height <= limit,
        "Export exceeds GPU texture limit {limit}; lower the resolution"
    );
    let ctx = egui::Context::default();
    ctx.input_mut(|input| input.max_texture_side = limit as usize);
    crate::text::FontBook::default().prepare(
        &ctx,
        project.levels[options.level]
            .layers
            .iter()
            .flat_map(|l| &l.items),
    );
    let mut textures = BTreeMap::new();
    let mut needed = std::collections::HashSet::new();
    if let Some(id) = &project.background_asset {
        needed.insert(id.as_str());
    }
    for layer in project.levels[options.level]
        .layers
        .iter()
        .filter(|l| l.visible)
    {
        for item in &layer.items {
            needed.extend(item.asset_ids());
        }
    }
    let mut texture_bytes = 0u64;
    let modes: BTreeMap<_, _> = library
        .iter()
        .map(|(id, a)| (id.clone(), a.meta.object_color_mode()))
        .collect();
    let mut variants = BTreeMap::new();
    for layer in project.levels[options.level]
        .layers
        .iter()
        .filter(|l| l.visible)
    {
        for item in &layer.items {
            if let ItemKind::Object { asset, .. } = &item.kind {
                if modes.get(asset) == Some(&ObjectColorMode::RedMask) {
                    variants.insert(
                        assets::color_texture_key(asset, item.tint),
                        (asset.as_str(), item.tint),
                    );
                    needed.remove(asset.as_str());
                }
            }
        }
    }
    for id in needed {
        let image = assets::decode_surface(&library[id])?;
        texture_bytes += image.as_raw().len() as u64;
        ensure!(
            texture_bytes <= 512 * 1024 * 1024,
            "Visible textures exceed export memory budget"
        );
        ensure!(
            image.width() <= limit && image.height() <= limit,
            "Asset exceeds GPU texture limit"
        );
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        textures.insert(
            id.to_owned(),
            ctx.load_texture(id, image, egui::TextureOptions::LINEAR_REPEAT),
        );
    }
    for (key, (id, tint)) in variants {
        let mut image = assets::decode_surface(&library[id])?;
        ensure!(
            image.width() <= limit && image.height() <= limit,
            "Asset exceeds GPU texture limit"
        );
        assets::recolor_red_mask(&mut image, tint);
        texture_bytes += image.as_raw().len() as u64;
        ensure!(
            texture_bytes <= 512 * 1024 * 1024,
            "Visible textures exceed export memory budget"
        );
        let pixels = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );
        textures.insert(
            key.clone(),
            ctx.load_texture(key, pixels, egui::TextureOptions::LINEAR),
        );
    }
    let size = Vec2::new(width as f32, height as f32);
    let rect = Rect::from_min_size(Pos2::ZERO, size);
    let blockers = crate::light::Blockers::new(project, options.level, library);
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            draw_scene_gpu(
                &painter,
                project,
                options.level,
                &SceneTextures {
                    textures: &textures,
                    modes: &modes,
                    sizes: None,
                    shader_masks: false,
                    blockers: Some(&blockers),
                },
                &View {
                    origin: Pos2::ZERO,
                    scale: options.pixels_per_grid as f32,
                    grid: options.grid,
                    lights: options.lights,
                },
            );
        },
    );
    let jobs = ctx.tessellate(output.shapes, 1.);
    let format = wgpu::TextureFormat::Rgba8Unorm;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Maploom export target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut renderer = egui_wgpu::Renderer::new(&gpu.device, format, None, 1, false);
    for (id, delta) in &output.textures_delta.set {
        renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
    }
    let desc = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [width, height],
        pixels_per_point: 1.,
    };
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Maploom export"),
        });
    let mut commands = renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &desc);
    {
        let view = texture.create_view(&Default::default());
        let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Maploom map"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        renderer.render(&mut pass.forget_lifetime(), &jobs, &desc);
    }
    let stride = (width * 4).div_ceil(256) * 256;
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Maploom readback"),
        size: stride as u64 * height as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    commands.push(encoder.finish());
    gpu.queue.submit(commands);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device.poll(wgpu::PollType::Wait)?;
    rx.recv()??;
    let data = buffer.slice(..).get_mapped_range();
    let mut bytes = Vec::with_capacity((width * height * 4) as usize);
    for row in data.chunks(stride as usize) {
        bytes.extend_from_slice(&row[..(width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    let mut image =
        image::RgbaImage::from_raw(width, height, bytes).context("Invalid GPU readback")?;
    // The GPU target is premultiplied; image encoders use straight alpha.
    for pixel in image.pixels_mut() {
        let alpha = pixel[3] as u32;
        if alpha > 0 && alpha < 255 {
            for c in &mut pixel.0[..3] {
                *c = ((*c as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
    Ok((image, format!("{} · {:?}", gpu.info.name, gpu.info.backend)))
}
pub fn export(
    path: &Path,
    project: &Project,
    library: &Library,
    options: ExportOptions,
) -> Result<String> {
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    let (data, renderer) = encode_export(project, library, options, &ext)?;
    assets::atomic_write(path, |f| {
        f.write_all(&data)?;
        Ok(())
    })?;
    Ok(renderer)
}

fn encode_export(
    project: &Project,
    library: &Library,
    options: ExportOptions,
    ext: &str,
) -> Result<(Vec<u8>, String)> {
    project.validate()?;
    ensure!(options.level < project.levels.len(), "Invalid export level");
    ensure!(
        matches!(ext, "png" | "webp" | "jpeg" | "jpg" | "svg" | "dd2vtt"),
        "Unsupported export extension"
    );
    ensure!(
        !matches!(ext, "jpeg" | "jpg") || options.level == project.main_level_index(),
        "Floor transparency requires PNG, WebP, SVG or VTT"
    );
    if ext == "svg" {
        let svg = svg(project, library, options)?;
        return Ok((svg.into_bytes(), "SVG vector export".into()));
    }
    let (image, gpu) = rasterize(project, library, options)?;
    let mut bytes = Cursor::new(vec![]);
    if ext == "jpg" || ext == "jpeg" {
        let rgb = image::DynamicImage::ImageRgba8(image).to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, options.jpeg_quality)
            .encode_image(&rgb)?;
    } else {
        image.write_to(
            &mut bytes,
            if ext == "webp" {
                image::ImageFormat::WebP
            } else {
                image::ImageFormat::Png
            },
        )?;
    }
    let mut data = bytes.into_inner();
    if ext == "dd2vtt" {
        let mut json = crate::formats::dd2vtt(
            project,
            options.level,
            options.pixels_per_grid,
            &data,
            library,
        );
        json["environment"]["baked_lighting"] = serde_json::Value::Bool(options.lights);
        data = serde_json::to_vec_pretty(&json)?;
    }
    Ok((data, gpu))
}

/// One registered image per floor in a single, atomically replaced ZIP archive.
pub fn export_all(
    path: &Path,
    project: &Project,
    library: &Library,
    options: ExportOptions,
    format: &str,
) -> Result<String> {
    export_floors(path, project, library, options, format, None)
}

fn export_floors(
    path: &Path,
    project: &Project,
    library: &Library,
    options: ExportOptions,
    format: &str,
    variation: Option<&str>,
) -> Result<String> {
    project.validate()?;
    ensure!(
        matches!(format, "png" | "webp" | "svg" | "dd2vtt"),
        "Floor transparency requires PNG, WebP, SVG or VTT"
    );
    let names = floor_export_names(project, format, variation);
    assets::atomic_write(path, |file| {
        let mut archive = zip::ZipWriter::new(file);
        let zip_options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (index, floor) in project.levels.iter().enumerate() {
            let (data, _) = encode_export(
                project,
                library,
                ExportOptions {
                    level: index,
                    ..options
                },
                format,
            )
                .with_context(|| format!("Could not export floor {}", floor.name))?;
            archive.start_file(names[index].as_str(), zip_options)?;
            archive.write_all(&data)?;
        }
        archive.finish()?;
        Ok(())
    })?;
    Ok(format!("{} floors exported", project.levels.len()))
}
/// Safe portable component, including Windows device names.
pub fn safe_export_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(64)
        .collect();
    let name = name.trim_matches([' ', '.']);
    if name.is_empty() {
        "Map".into()
    } else if matches!(
        name.split('.')
            .next()
            .unwrap_or("")
            .to_ascii_uppercase()
            .as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        format!("_{name}")
    } else {
        name.into()
    }
}

pub fn variation_archive_names(project: &Project) -> Vec<String> {
    variation_export_names(project, "zip")
}

fn variation_export_names(project: &Project, extension: &str) -> Vec<String> {
    let map = safe_export_name(&project.name);
    let names: Vec<_> = if project.variations.is_empty() {
        vec!["Variation 1".into()]
    } else {
        project
            .variations
            .iter()
            .map(|v| safe_export_name(&v.name))
            .collect()
    };
    unique_export_names(
        names.into_iter().map(|name| format!("{map}-{name}")),
        extension,
    )
}

pub fn floor_export_names(
    project: &Project,
    extension: &str,
    variation: Option<&str>,
) -> Vec<String> {
    let map = safe_export_name(&project.name);
    let variation = variation.map(safe_export_name);
    unique_export_names(
        project.levels.iter().map(|floor| {
            let floor = safe_export_name(&floor.name);
            match &variation {
                Some(variation) => format!("{map}-{floor}-{variation}"),
                None => format!("{map}-{floor}"),
            }
        }),
        extension,
    )
}

fn unique_export_names(names: impl IntoIterator<Item=String>, extension: &str) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    names
        .into_iter()
        .map(|base| {
            let mut stem = base.clone();
            let mut number = 2;
            while !used.insert(stem.to_lowercase()) {
                stem = format!("{base}-{number}");
                number += 1;
            }
            format!("{stem}.{extension}")
        })
        .collect()
}

pub fn validate_variation_export(project: &Project, floors: bool, format: &str) -> Result<()> {
    project.validate()?;
    ensure!(
        matches!(format, "png" | "webp" | "jpeg" | "svg" | "dd2vtt"),
        "Unsupported export format"
    );
    ensure!(
        !floors || format != "jpeg",
        "Floor transparency requires PNG, WebP, SVG or VTT"
    );
    Ok(())
}

/// With floors, `path` is a directory; otherwise it is the single ZIP file.
/// Variation-only exports use each variation's main floor, independently of
/// the active editor floor (the variations can have different floor counts).
pub fn export_variations(
    path: &Path,
    project: &Project,
    library: &Library,
    options: ExportOptions,
    format: &str,
    floors: bool,
) -> Result<String> {
    validate_variation_export(project, floors, format)?;
    let scenes: Vec<_> = if project.variations.is_empty() {
        vec![("Variation 1".to_owned(), project.clone())]
    } else {
        project
            .variations
            .iter()
            .map(|v| (v.name.clone(), project.variation_scene(v)))
            .collect()
    };
    if floors {
        // Stage every archive first: a rendering failure cannot replace earlier outputs.
        std::fs::create_dir_all(path)?;
        let staging = tempfile::tempdir_in(path)?;
        let names = variation_archive_names(project);
        for ((name, scene), file) in scenes.iter().zip(&names) {
            export_floors(
                &staging.path().join(file),
                scene,
                library,
                options,
                format,
                Some(name),
            )
                .with_context(|| format!("Could not export variation {name}"))?;
        }
        for file in &names {
            let mut source = std::fs::File::open(staging.path().join(file))?;
            assets::atomic_write(&path.join(file), |target| {
                std::io::copy(&mut source, target)?;
                Ok(())
            })?;
        }
    } else {
        let names = variation_export_names(project, format);
        assets::atomic_write(path, |file| {
            let mut archive = zip::ZipWriter::new(file);
            let zip_options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for ((name, scene), file_name) in scenes.iter().zip(&names) {
                let (data, _) = encode_export(
                    scene,
                    library,
                    ExportOptions {
                        level: scene.main_level_index(),
                        ..options
                    },
                    format,
                )
                    .with_context(|| format!("Could not export variation {name}"))?;
                archive.start_file(file_name.as_str(), zip_options)?;
                archive.write_all(&data)?;
            }
            archive.finish()?;
            Ok(())
        })?;
    }
    Ok(format!("{} variations exported", scenes.len()))
}

struct Svg<'a> {
    out: String,
    library: &'a Library,
    images: BTreeMap<String, String>,
    next: usize,
}
impl<'a> Svg<'a> {
    fn textured_path(&mut self, item: &Item, asset: &str) -> Result<()> {
        self.textured_ribbon(&crate::paths::item_ribbon(item), asset)
    }
    fn textured_ribbon(
        &mut self,
        vertices: &[crate::paths::PathVertex],
        asset: &str,
    ) -> Result<()> {
        // Each triangle maps a shared UV pattern to the same ribbon used by WebGL.
        // Linear vertex opacity is represented by a gradient in map coordinates.
        let pattern = self.pattern(asset, 1., 1.)?;
        // Rasterize shared triangle edges without antialiasing seams, then clip
        // the complete ribbon once to retain its smooth outer silhouette.
        let clip = format!("pathclip{}", self.next);
        self.next += 1;
        self.out
            .push_str(&format!("<defs><clipPath id=\"{clip}\"><path d=\""));
        for n in (2..vertices.len()).step_by(2) {
            let ps = [n - 2, n, n + 1, n - 1].map(|i| vertices[i].position);
            self.out.push_str(&format!(
                "M{} {} L{} {} L{} {} L{} {} Z ",
                ps[0].x, ps[0].y, ps[1].x, ps[1].y, ps[2].x, ps[2].y, ps[3].x, ps[3].y
            ));
        }
        self.out.push_str(&format!(
            "\"/></clipPath></defs><g clip-path=\"url(#{clip})\"><g shape-rendering=\"crispEdges\">"
        ));
        for n in (2..vertices.len()).step_by(2) {
            for indices in [[n - 2, n, n + 1], [n - 2, n + 1, n - 1]] {
                let [a, b, c] = indices.map(|i| vertices[i]);
                let (ux, uy, vx, vy) = (
                    b.uv.x - a.uv.x,
                    b.uv.y - a.uv.y,
                    c.uv.x - a.uv.x,
                    c.uv.y - a.uv.y,
                );
                let det = ux * vy - vx * uy;
                let cross = (b.position.x - a.position.x) * (c.position.y - a.position.y)
                    - (c.position.x - a.position.x) * (b.position.y - a.position.y);
                if det.abs() < 1e-9 || cross.abs() < 1e-9 {
                    continue;
                }
                let x =
                    ((b.position.x - a.position.x) * vy - (c.position.x - a.position.x) * uy) / det;
                let y =
                    ((b.position.y - a.position.y) * vy - (c.position.y - a.position.y) * uy) / det;
                let z =
                    ((c.position.x - a.position.x) * ux - (b.position.x - a.position.x) * vx) / det;
                let w =
                    ((c.position.y - a.position.y) * ux - (b.position.y - a.position.y) * vx) / det;
                let tx = a.position.x - x * a.uv.x - z * a.uv.y;
                let ty = a.position.y - y * a.uv.x - w * a.uv.y;
                let id = format!("path{}", self.next);
                self.next += 1;
                self.out.push_str(&format!("<defs><pattern id=\"{id}\" href=\"#{pattern}\" patternTransform=\"matrix({x} {y} {z} {w} {tx} {ty})\"/>"));
                let da = b.alpha - a.alpha;
                let db = c.alpha - a.alpha;
                let gx = (da * (c.position.y - a.position.y) - db * (b.position.y - a.position.y))
                    / cross;
                let gy = (db * (b.position.x - a.position.x) - da * (c.position.x - a.position.x))
                    / cross;
                let g2 = gx * gx + gy * gy;
                if g2 > 1e-10 {
                    let x0 = a.position.x - gx * a.alpha / g2;
                    let y0 = a.position.y - gy * a.alpha / g2;
                    let x1 = x0 + gx / g2;
                    let y1 = y0 + gy / g2;
                    self.out.push_str(&format!("<linearGradient id=\"{id}a\" gradientUnits=\"userSpaceOnUse\" x1=\"{x0}\" y1=\"{y0}\" x2=\"{x1}\" y2=\"{y1}\"><stop stop-color=\"black\"/><stop offset=\"1\" stop-color=\"white\"/></linearGradient><mask id=\"{id}m\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><polygon points=\"{},{} {},{} {},{}\" fill=\"url(#{id}a)\"/></mask>",
                                               a.position.x.min(b.position.x).min(c.position.x), a.position.y.min(b.position.y).min(c.position.y),
                                               a.position.x.max(b.position.x).max(c.position.x) - a.position.x.min(b.position.x).min(c.position.x),
                                               a.position.y.max(b.position.y).max(c.position.y) - a.position.y.min(b.position.y).min(c.position.y),
                                               a.position.x, a.position.y, b.position.x, b.position.y, c.position.x, c.position.y));
                }
                self.out.push_str("</defs>");
                let opacity = if g2 > 1e-10 {
                    format!(" mask=\"url(#{id}m)\"")
                } else {
                    format!(" opacity=\"{}\"", a.alpha)
                };
                self.out.push_str(&format!(
                    "<polygon points=\"{},{} {},{} {},{}\" fill=\"url(#{id})\"{opacity}/>",
                    a.position.x,
                    a.position.y,
                    b.position.x,
                    b.position.y,
                    c.position.x,
                    c.position.y
                ));
            }
        }
        self.out.push_str("</g></g>");
        Ok(())
    }
    fn image(&mut self, id: &str) -> Result<String> {
        if let Some(data) = self.images.get(id) {
            return Ok(data.clone());
        }
        let image = assets::decode_surface(&self.library[id])?;
        let mut png = Cursor::new(vec![]);
        image.write_to(&mut png, image::ImageFormat::Png)?;
        let data = STANDARD.encode(png.into_inner());
        self.images.insert(id.into(), data.clone());
        Ok(data)
    }
    fn recolored_image(&mut self, asset: &str, tint: Color) -> Result<String> {
        let key = assets::color_texture_key(asset, tint);
        if let Some(data) = self.images.get(&key) {
            return Ok(data.clone());
        }
        let mut image = assets::decode(&assets::verified_bytes(&self.library[asset])?)?;
        assets::recolor_red_mask(&mut image, tint);
        let mut png = Cursor::new(vec![]);
        image.write_to(&mut png, image::ImageFormat::Png)?;
        let data = STANDARD.encode(png.into_inner());
        self.images.insert(key, data.clone());
        Ok(data)
    }
    fn pattern(&mut self, asset: &str, w: f32, h: f32) -> Result<String> {
        let id = format!("p{}", self.next);
        self.next += 1;
        let data = self.image(asset)?;
        self.out.push_str(&format!("<defs><pattern id=\"{id}\" patternUnits=\"userSpaceOnUse\" width=\"{w}\" height=\"{h}\"><image width=\"{w}\" height=\"{h}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{data}\"/></pattern></defs>"));
        Ok(id)
    }
    fn tint(&mut self, c: Color) -> Option<String> {
        if c == [255; 4] {
            return None;
        }
        let id = format!("t{}", self.next);
        self.next += 1;
        let (r, g, b, a) = (
            c[0] as f32 / 255.,
            c[1] as f32 / 255.,
            c[2] as f32 / 255.,
            c[3] as f32 / 255.,
        );
        self.out.push_str(&format!("<defs><filter id=\"{id}\" color-interpolation-filters=\"sRGB\"><feColorMatrix values=\"{r} 0 0 0 0 0 {g} 0 0 0 0 0 {b} 0 0 0 0 0 {a} 0\"/></filter></defs><g filter=\"url(#{id})\">"));
        Some(id)
    }
    fn fill(&mut self, rings: &[Vec<Point>], asset: &str, scale: f32) -> Result<()> {
        let id = self.pattern(asset, scale, scale)?;
        let mut path = String::new();
        for ring in rings {
            if ring.len() < 3 {
                continue;
            }
            path.push_str(&format!("M{} {}", ring[0].x, ring[0].y));
            for p in &ring[1..] {
                path.push_str(&format!(" L{} {}", p.x, p.y));
            }
            path.push_str(" Z ");
        }
        self.out.push_str(&format!(
            "<path d=\"{path}\" fill=\"url(#{id})\" fill-rule=\"evenodd\"/>"
        ));
        Ok(())
    }
    fn wall(&mut self, lines: &[[Point; 2]], asset: &str, width: f32, scale: f32) -> Result<()> {
        self.wall_sections(lines, asset, width, scale, None)
    }
    fn cave_wall(
        &mut self,
        rings: &[Vec<Point>],
        asset: &str,
        width: f32,
        scale: f32,
    ) -> Result<()> {
        for ring in rings {
            self.textured_ribbon(&crate::caves::wall_ribbon(ring, width, scale), asset)?;
        }
        Ok(())
    }
    fn wall_sections(
        &mut self,
        lines: &[[Point; 2]],
        asset: &str,
        width: f32,
        scale: f32,
        openings: Option<&Openings>,
    ) -> Result<()> {
        let id = self.pattern(asset, scale, width)?;
        for line in lines {
            let a = line[0];
            let b = line[1];
            let length = a.distance(b);
            let angle = (b.y - a.y).atan2(b.x - a.x).to_degrees();
            let spans = openings.map_or_else(|| vec![(0., 1.)], |o| o.spans(*line));
            for (lo, hi) in spans {
                let x = lo * length;
                let length = (hi - lo) * length;
                self.out.push_str(&format!("<g transform=\"translate({} {}) rotate({angle}) translate(0 {})\"><rect x=\"{x}\" width=\"{length}\" height=\"{width}\" fill=\"url(#{id})\"/></g>", a.x, a.y, -width / 2.));
            }
        }
        Ok(())
    }
    fn object(
        &mut self,
        data: String,
        position: Point,
        size: Point,
        pivot: Point,
        rotation: f32,
        flip: [bool; 2],
    ) -> Result<()> {
        let sx = if flip[0] { -1. } else { 1. };
        let sy = if flip[1] { -1. } else { 1. };
        let tx = if flip[0] { size.x } else { 0. };
        let ty = if flip[1] { size.y } else { 0. };
        self.out.push_str(&format!("<g transform=\"translate({} {}) rotate({}) translate({} {})\"><image width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" transform=\"translate({tx} {ty}) scale({sx} {sy})\" href=\"data:image/png;base64,{data}\"/></g>", position.x, position.y, rotation.to_degrees(), -pivot.x * size.x, -pivot.y * size.y, size.x, size.y));
        Ok(())
    }
}
pub fn svg(project: &Project, library: &Library, options: ExportOptions) -> Result<String> {
    project.validate()?;
    ensure!(options.level < project.levels.len(), "Invalid level");
    svg_surface(
        &project.export_surface(options.level),
        library,
        ExportOptions {
            level: 0,
            ..options
        },
    )
}

fn svg_surface(project: &Project, library: &Library, options: ExportOptions) -> Result<String> {
    let mut resolved = crate::storage::prefabs::expanded(project).into_owned();
    crate::formats::resolve_dd_assets(&mut resolved, library)?;
    let project = &resolved;
    project.validate()?;
    ensure!(options.level < project.levels.len(), "Invalid level");
    assets::require_assets(project, options.level, library)?;
    let mut svg = Svg {
        out: format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\"><defs><clipPath id=\"map\"><rect width=\"{}\" height=\"{}\"/></clipPath></defs><g clip-path=\"url(#map)\">",
            project.width * options.pixels_per_grid,
            project.height * options.pixels_per_grid,
            project.width,
            project.height,
            project.width,
            project.height
        ),
        library,
        images: BTreeMap::new(),
        next: 0,
    };
    let ambient_tinted = options.lights
        && svg
        .tint([
            project.ambient_light[0],
            project.ambient_light[1],
            project.ambient_light[2],
            255,
        ])
        .is_some();
    svg.out.push_str(&format!(
        "<rect width=\"{}\" height=\"{}\" fill=\"#{:02x}{:02x}{:02x}\" fill-opacity=\"{}\"/>",
        project.width,
        project.height,
        project.background[0],
        project.background[1],
        project.background[2],
        project.background[3] as f32 / 255.
    ));
    if let Some(id) = &project.background_asset {
        svg.fill(
            &[vec![
                Point::new(0., 0.),
                Point::new(project.width as f32, 0.),
                Point::new(project.width as f32, project.height as f32),
                Point::new(0., project.height as f32),
            ]],
            id,
            library[id].meta.size.x,
        )?;
    }
    let openings = Openings::new(&project.levels[options.level]);
    let cave_rooms = crate::caves::rooms(&project.levels[options.level]);
    let blockers = crate::light::Blockers::new(project, options.level, library);
    let passes = landscape_passes(&project.levels[options.level]);
    for pass in 0..passes {
        for layer in project.levels[options.level]
            .layers
            .iter()
            .filter(|l| l.visible)
        {
            svg.out
                .push_str(&format!("<g opacity=\"{}\">", layer.opacity));
            for (index, item) in layer.items.iter().enumerate() {
                if passes > 1 && landscape_pass(item) != pass {
                    continue;
                }
                let openings = openings.for_wall(&item.id);
                let mode = if let ItemKind::Object { asset, .. } = &item.kind {
                    library[asset].meta.object_color_mode()
                } else {
                    ObjectColorMode::Tint
                };
                let tinted = mode == ObjectColorMode::Tint
                    && !matches!(item.kind, ItemKind::Light { .. } | ItemKind::Text { .. })
                    && svg.tint(item.tint).is_some();
                match &item.kind {
                    ItemKind::Room {
                        rings,
                        floor,
                        wall,
                        width,
                    } => {
                        if let Some(id) = floor {
                            let tinted = svg.tint(item.floor_tint).is_some();
                            svg.fill(rings, id, item.texture_scale)?;
                            if tinted {
                                svg.out.push_str("</g>");
                            }
                        }
                        if let Some(id) = wall {
                            let tinted = svg.tint(item.wall_tint).is_some();
                            svg.wall_sections(
                                &item.wall_segments(),
                                id,
                                *width,
                                library[id].meta.size.x,
                                Some(&openings),
                            )?;
                            if tinted {
                                svg.out.push_str("</g>");
                            }
                        }
                    }
                    ItemKind::Roof {
                        points,
                        width,
                        roof_type,
                        ridge,
                        edge,
                        hip,
                        shade,
                        sun_direction,
                        contrast,
                    } => {
                        let roof = crate::roofs::build(points, *width, *roof_type);
                        if let Some(id) = &item.texture {
                            for facet in &roof.facets {
                                let brightness =
                                    (facet.brightness(*shade, *sun_direction, *contrast) * 255.)
                                        as u8;
                                let shaded = svg
                                    .tint([brightness, brightness, brightness, 255])
                                    .is_some();
                                let angle = facet.along.y.atan2(facet.along.x).to_degrees();
                                let sign =
                                    facet.along.x * facet.down.y - facet.along.y * facet.down.x;
                                let pattern =
                                    svg.pattern(id, item.texture_scale, item.texture_scale)?;
                                // Pattern coordinates match the per-facet GPU UVs, including reflected slopes.
                                let transformed = format!("p{}", svg.next);
                                svg.next += 1;
                                svg.out.push_str(&format!("<defs><pattern id=\"{transformed}\" href=\"#{pattern}\" patternTransform=\"translate({} {}) rotate({angle}) scale(1 {sign})\"/></defs>", facet.origin.x, facet.origin.y));
                                let pts = facet
                                    .points
                                    .iter()
                                    .map(|p| format!("{},{}", p.x, p.y))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                svg.out.push_str(&format!(
                                    "<polygon points=\"{pts}\" fill=\"url(#{transformed})\"/>"
                                ));
                                if shaded {
                                    svg.out.push_str("</g>");
                                }
                            }
                        }
                        let edges: Vec<_> = roof
                            .outline
                            .iter()
                            .enumerate()
                            .map(|(n, p)| [*p, roof.outline[(n + 1) % roof.outline.len()]])
                            .collect();
                        let ridges: Vec<_> = roof.ridge.windows(2).map(|p| [p[0], p[1]]).collect();
                        for (id, lines) in [(edge, &edges), (hip, &roof.hips), (ridge, &ridges)] {
                            if let Some(id) = id {
                                let size = library[id].meta.size;
                                let factor = item.texture_scale
                                    / item
                                    .texture
                                    .as_ref()
                                    .map(|id| library[id].meta.size.x)
                                    .unwrap_or(4.)
                                    .max(0.01);
                                svg.wall(lines, id, (size.y * factor).max(0.015), size.x * factor)?;
                            }
                        }
                    }
                    ItemKind::Floor { .. } => {
                        if let Some(id) = &item.texture {
                            svg.fill(&item.floor_rings(), id, item.texture_scale)?;
                        }
                    }
                    ItemKind::Path { .. } => {
                        if let Some(id) = &item.texture {
                            svg.textured_path(item, id)?;
                        }
                    }
                    ItemKind::Wall { width, .. } => {
                        if let Some(id) = &item.texture {
                            svg.wall_sections(
                                &item.wall_segments(),
                                id,
                                *width,
                                item.texture_scale,
                                Some(&openings),
                            )?;
                        }
                    }
                    ItemKind::TerrainRegion { rings, .. } => {
                        if let Some(cave) = &item.cave {
                            if let Some(id) = &item.texture {
                                let tinted = svg
                                    .tint(if cave.colorable {
                                        cave.ground_color
                                    } else {
                                        [255; 4]
                                    })
                                    .is_some();
                                let floor = item
                                    .border
                                    .as_ref()
                                    .and_then(|id| library.get(id))
                                    .map(|a| {
                                        crate::caves::render_floor(item, &cave_rooms, a.meta.size.y)
                                    })
                                    .unwrap_or_else(|| crate::caves::floor(item, &cave_rooms));
                                svg.fill(&floor, id, item.texture_scale)?;
                                if tinted {
                                    svg.out.push_str("</g>");
                                }
                            }
                            if let Some(id) = &item.border {
                                let tinted = svg
                                    .tint(if cave.colorable {
                                        cave.wall_color
                                    } else {
                                        [255; 4]
                                    })
                                    .is_some();
                                let meta = &library[id].meta;
                                let masks = crate::geometry::union_rooms(&[
                                    cave.blasts.clone(),
                                    cave_rooms.clone(),
                                ])
                                    .unwrap_or_default();
                                let clip = format!("cavecut{}", svg.next);
                                svg.next += 1;
                                let mut path =
                                    format!("M0 0 H{} V{} H0 Z ", project.width, project.height);
                                for ring in &masks {
                                    if ring.is_empty() {
                                        continue;
                                    }
                                    path.push_str(&format!("M{} {} ", ring[0].x, ring[0].y));
                                    for p in &ring[1..] {
                                        path.push_str(&format!("L{} {} ", p.x, p.y));
                                    }
                                    path.push_str("Z ");
                                }
                                svg.out.push_str(&format!("<defs><clipPath id=\"{clip}\"><path d=\"{path}\" clip-rule=\"evenodd\"/></clipPath></defs><g clip-path=\"url(#{clip})\">"));
                                svg.cave_wall(rings, id, meta.size.y, meta.size.x)?;
                                svg.out.push_str("</g>");
                                if tinted {
                                    svg.out.push_str("</g>");
                                }
                            }
                        } else if let Some(water) = &item.water {
                            let water = crate::water::rasterize(
                                rings,
                                water,
                                options.pixels_per_grid as f32,
                            );
                            let mut png = Cursor::new(Vec::new());
                            water.image.write_to(&mut png, image::ImageFormat::Png)?;
                            svg.out.push_str(&format!("<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{}\"/>",
                                                      water.origin.x, water.origin.y, water.size.x, water.size.y, STANDARD.encode(png.into_inner())));
                        } else {
                            if let Some(id) = &item.texture {
                                svg.fill(rings, id, item.texture_scale)?;
                            }
                            if let Some(id) = &item.border {
                                let meta = &library[id].meta;
                                let lines = rings
                                    .iter()
                                    .flat_map(|r| {
                                        (0..r.len()).map(|i| [r[i], r[(i + 1) % r.len()]])
                                    })
                                    .collect::<Vec<_>>();
                                svg.wall(
                                    &lines,
                                    id,
                                    meta.size.y.clamp(0.08, 2.),
                                    meta.size.x.max(0.1),
                                )?;
                            }
                        }
                    }
                    ItemKind::Terrain { .. } => {
                        if let Some(run) = terrain_run(&layer.items, index) {
                            let stamps: Vec<_> = run
                                .iter()
                                .filter_map(|i| {
                                    if let ItemKind::Terrain {
                                        position, radius, ..
                                    } = i.kind
                                    {
                                        Some((position, radius))
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            let rings = crate::geometry::terrain_contours(&stamps);
                            if let Some(id) = &item.texture {
                                svg.fill(&rings, id, item.texture_scale)?;
                            }
                            if let Some(id) = &item.border {
                                let meta = &library[id].meta;
                                let lines = rings
                                    .iter()
                                    .flat_map(|r| {
                                        (0..r.len()).map(|i| [r[i], r[(i + 1) % r.len()]])
                                    })
                                    .collect::<Vec<_>>();
                                svg.wall(
                                    &lines,
                                    id,
                                    meta.size.y.clamp(0.08, 2.),
                                    meta.size.x.max(0.1),
                                )?;
                            }
                        }
                    }
                    ItemKind::Object {
                        asset,
                        position,
                        size,
                        pivot,
                        rotation,
                        flip_x,
                        flip_y,
                    } => {
                        let data = if mode == ObjectColorMode::RedMask {
                            svg.recolored_image(asset, item.tint)?
                        } else {
                            svg.image(asset)?
                        };
                        svg.object(
                            data,
                            *position,
                            *size,
                            *pivot,
                            *rotation,
                            [*flip_x, *flip_y],
                        )?;
                    }
                    ItemKind::Portal { bounds, .. } => {
                        if let Some(id) = &item.texture {
                            let image = assets::decode_surface(&library[id])?;
                            let length = bounds[0].distance(bounds[1]);
                            let size = Point::new(
                                length,
                                length * image.height() as f32 / image.width() as f32,
                            );
                            let position = Point::new(
                                (bounds[0].x + bounds[1].x) / 2.,
                                (bounds[0].y + bounds[1].y) / 2.,
                            );
                            let angle =
                                (bounds[1].y - bounds[0].y).atan2(bounds[1].x - bounds[0].x);
                            let data = svg.image(id)?;
                            svg.object(
                                data,
                                position,
                                size,
                                Point::new(0.5, 0.5),
                                angle,
                                [false, false],
                            )?;
                        }
                    }
                    ItemKind::Text { .. } => svg
                        .out
                        .push_str(&crate::text::svg(item, project.pixels_per_grid as f32)),
                    _ => {}
                }
                if tinted {
                    svg.out.push_str("</g>");
                }
            }
            svg.out.push_str("</g>");
        }
    }
    if ambient_tinted {
        svg.out.push_str("</g>");
    }
    if options.lights {
        for layer in project.levels[options.level]
            .layers
            .iter()
            .filter(|l| l.visible)
        {
            svg.out
                .push_str(&format!("<g opacity=\"{}\">", layer.opacity));
            for item in &layer.items {
                if let ItemKind::Light {
                    position,
                    range,
                    intensity,
                    smoothing,
                    color,
                    shadows,
                } = &item.kind
                {
                    let visibility = if *shadows {
                        blockers.visibility(*position, *range)
                    } else {
                        None
                    };
                    if let Some(polygon) = &visibility {
                        let id = format!("visibility{}", svg.next);
                        svg.next += 1;
                        let points = polygon
                            .iter()
                            .map(|p| format!("{},{}", p.x, p.y))
                            .collect::<Vec<_>>()
                            .join(" ");
                        svg.out.push_str(&format!("<defs><clipPath id=\"{id}\" clipPathUnits=\"userSpaceOnUse\"><polygon points=\"{points}\"/></clipPath></defs><g clip-path=\"url(#{id})\">"));
                    }
                    let side = (2. * range * options.pixels_per_grid as f32)
                        .ceil()
                        .clamp(32., 2048.) as u32;
                    let mut image = crate::light::rasterize(*smoothing, *intensity, side);
                    for pixel in image.pixels_mut() {
                        pixel.0 = multiply_colors(pixel.0, *color);
                    }
                    let mut png = Cursor::new(vec![]);
                    image.write_to(&mut png, image::ImageFormat::Png)?;
                    let data = STANDARD.encode(png.into_inner());
                    svg.out.push_str(&format!("<image x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{data}\"/>", position.x - range, position.y - range, range * 2., range * 2.));
                    if visibility.is_some() {
                        svg.out.push_str("</g>");
                    }
                }
            }
            svg.out.push_str("</g>");
        }
    }
    if options.grid {
        for x in 0..=project.width {
            svg.out.push_str(&format!(
                "<path d=\"M{x} 0 V{}\" stroke=\"black\" opacity=\"0.14\" stroke-width=\"0.01\"/>",
                project.height
            ));
        }
        for y in 0..=project.height {
            svg.out.push_str(&format!(
                "<path d=\"M0 {y} H{}\" stroke=\"black\" opacity=\"0.14\" stroke-width=\"0.01\"/>",
                project.width
            ));
        }
    }
    svg.out.push_str("</g></svg>");
    Ok(svg.out)
}

pub fn draw_selection(painter: &egui::Painter, view: &View, item: &Item, accent: Color32) {
    let stroke = Stroke::new(1.5, accent);
    match &item.kind {
        ItemKind::Roof {
            points,
            width,
            roof_type,
            ..
        } => {
            painter.add(egui::Shape::closed_line(
                crate::roofs::build(points, *width, *roof_type)
                    .outline
                    .iter()
                    .map(|p| view.screen(*p))
                    .collect(),
                stroke,
            ));
        }
        ItemKind::Object {
            position,
            size,
            pivot,
            rotation,
            ..
        } => {
            painter.add(egui::Shape::closed_line(
                object_corners(*position, *size, *pivot, *rotation)
                    .iter()
                    .map(|p| view.screen(*p))
                    .collect(),
                stroke,
            ));
        }
        ItemKind::Room { rings, .. } | ItemKind::TerrainRegion { rings, .. } => {
            for ring in rings {
                painter.add(egui::Shape::closed_line(
                    ring.iter().map(|p| view.screen(*p)).collect(),
                    stroke,
                ));
            }
        }
        ItemKind::Floor { points, .. } => {
            painter.add(egui::Shape::closed_line(
                points.iter().map(|p| view.screen(*p)).collect(),
                stroke,
            ));
        }
        ItemKind::Terrain {
            position, radius, ..
        }
        | ItemKind::Light {
            position,
            range: radius,
            ..
        } => {
            painter.circle_stroke(view.screen(*position), radius * view.scale, stroke);
        }
        _ => {
            let points = item.points();
            if points.len() > 1 {
                painter.add(egui::Shape::line(
                    points.iter().map(|p| view.screen(*p)).collect(),
                    stroke,
                ));
            } else if let Some(p) = points.first() {
                painter.circle_stroke(view.screen(*p), 5., stroke);
            }
        }
    }
}
