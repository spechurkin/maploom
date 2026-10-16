//! Non-destructive cave/room composition. Blasting cuts floor and rock without
//! creating a new rock boundary; rooms always supply the overlapping floor.
use crate::document::{Item, ItemKind, Level, Point};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::single::SingleFloatOverlay,
};

pub fn difference(subject: &[Vec<Point>], clip: &[Vec<Point>]) -> Vec<Vec<Point>> {
    if clip.is_empty() {
        return subject.to_vec();
    }
    let contours = |rings: &[Vec<Point>]| {
        rings
            .iter()
            .map(|r| {
                r.iter()
                    .map(|p| [p.x as f64, p.y as f64])
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    contours(subject)
        .overlay(&contours(clip), OverlayRule::Difference, FillRule::EvenOdd)
        .into_iter()
        .flatten()
        .map(|r| {
            r.into_iter()
                .map(|p| Point::new(p[0] as f32, p[1] as f32))
                .collect()
        })
        .collect()
}

pub fn rooms(level: &Level) -> Vec<Vec<Point>> {
    crate::geometry::union_rooms(
        &level
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
            .filter_map(|i| {
                if let ItemKind::Room { rings, .. } = &i.kind {
                    Some(rings.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default()
}

pub fn footprint(item: &Item) -> Vec<Vec<Point>> {
    if let (Some(cave), ItemKind::TerrainRegion { rings, .. }) = (&item.cave, &item.kind) {
        difference(rings, &cave.blasts)
    } else {
        vec![]
    }
}

pub fn floor(item: &Item, rooms: &[Vec<Point>]) -> Vec<Vec<Point>> {
    difference(&footprint(item), rooms)
}

pub fn edges(rings: &[Vec<Point>]) -> Vec<[Point; 2]> {
    rings
        .iter()
        .flat_map(|r| {
            r.iter()
                .enumerate()
                .map(|(n, p)| [*p, r[(n + 1) % r.len()]])
        })
        .collect()
}

/// Rock artwork has its own silhouette. Filter sub-width contour steps before
/// offsetting it: miters at those steps otherwise fold the texture over itself.
/// The visible floor shares this contour; saved geometry, blasts and LOS stay exact.
pub fn wall_contour(ring: &[Point], width: f32) -> Vec<Point> {
    if ring.len() < 3 || !width.is_finite() || width <= 0. {
        return vec![];
    }
    let lengths: Vec<_> = ring
        .iter()
        .enumerate()
        .map(|(n, p)| p.distance(ring[(n + 1) % ring.len()]))
        .collect();
    let perimeter: f32 = lengths.iter().sum();
    if perimeter < 1e-5 {
        return vec![];
    }
    let count = (perimeter / (width / 8.).min(0.08))
        .ceil()
        .clamp(8., 16384.) as usize;
    let spacing = perimeter / count as f32;
    let (mut edge, mut distance) = (0, 0.);
    let samples: Vec<_> = (0..count)
        .map(|n| {
            let d = n as f32 * spacing;
            while edge + 1 < ring.len() && distance + lengths[edge] < d {
                distance += lengths[edge];
                edge += 1;
            }
            crate::openings::at(
                [ring[edge], ring[(edge + 1) % ring.len()]],
                (d - distance) / lengths[edge].max(1e-7),
            )
        })
        .collect();
    let reach = ((width * 1.5 / spacing).ceil() as usize).clamp(1, count / 4);
    let weights: Vec<_> = (0..=reach * 2)
        .map(|n| (-2. * ((n as f32 - reach as f32) / reach as f32).powi(2)).exp())
        .collect();
    let total: f32 = weights.iter().sum();
    (0..count)
        .map(|n| {
            let mut p = Point::new(0., 0.);
            for (j, w) in weights.iter().enumerate() {
                let q = samples[(n + j + count - reach) % count];
                p.x += q.x * w / total;
                p.y += q.y * w / total;
            }
            p
        })
        .collect()
}

/// Fill to the same centerline as the rock strip, before cutting openings.
/// Keeping the old floor contour would expose square corners and concave gaps.
pub fn render_floor(item: &Item, rooms: &[Vec<Point>], width: f32) -> Vec<Vec<Point>> {
    let (Some(cave), ItemKind::TerrainRegion { rings, .. }) = (&item.cave, &item.kind) else {
        return vec![];
    };
    let rings: Vec<_> = rings
        .iter()
        .map(|r| wall_contour(r, width))
        .filter(|r| r.len() >= 3)
        .collect();
    difference(&difference(&rings, &cave.blasts), rooms)
}

pub fn wall_ribbon(ring: &[Point], width: f32, scale: f32) -> Vec<crate::paths::PathVertex> {
    if !scale.is_finite() || scale <= 0. {
        return vec![];
    }
    let points = wall_contour(ring, width);
    let count = points.len();
    if count < 3 {
        return vec![];
    }
    let mut out = Vec::with_capacity((count + 1) * 2);
    let mut distance = 0.;
    for n in 0..=count {
        let p = points[n % count];
        if n > 0 {
            distance += points[(n - 1) % count].distance(p);
        }
        let a = points[(n + count - 1) % count];
        let b = points[(n + 1) % count];
        let length = a.distance(b).max(1e-7);
        let normal = Point::new(-(b.y - a.y) / length, (b.x - a.x) / length);
        for (sign, v) in [(1., 1.), (-1., 0.)] {
            out.push(crate::paths::PathVertex {
                position: Point::new(
                    p.x + normal.x * width / 2. * sign,
                    p.y + normal.y * width / 2. * sign,
                ),
                uv: Point::new(distance / scale, v),
                alpha: 1.,
            });
        }
    }
    out
}

/// Exact edge intersections partition a segment before classifying its spans.
/// Include boundary for cave scenery; rooms retain coincident exterior walls.
pub fn outside(line: [Point; 2], masks: &[Vec<Point>], boundary: bool) -> Vec<(f32, f32)> {
    if masks.is_empty() {
        return vec![(0., 1.)];
    }
    let [a, b] = line;
    let (dx, dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    let len2 = dx * dx + dy * dy;
    if len2 < 1e-14 {
        return vec![];
    }
    let mut cuts = vec![0_f64, 1.];
    let edges = edges(masks);
    for &[c, d] in &edges {
        let (ex, ey) = ((d.x - c.x) as f64, (d.y - c.y) as f64);
        let (cx, cy) = ((c.x - a.x) as f64, (c.y - a.y) as f64);
        let den = dx * ey - dy * ex;
        if den.abs() > 1e-12 {
            let t = (cx * ey - cy * ex) / den;
            let u = (cx * dy - cy * dx) / den;
            if (0. ..=1.).contains(&t) && (-1e-8..=1. + 1e-8).contains(&u) {
                cuts.push(t);
            }
        } else if (cx * dy - cy * dx).abs() < 1e-8 {
            cuts.extend([
                ((cx * dx + cy * dy) / len2).clamp(0., 1.),
                (((d.x - a.x) as f64 * dx + (d.y - a.y) as f64 * dy) / len2).clamp(0., 1.),
            ]);
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    cuts.windows(2)
        .filter_map(|pair| {
            let p = crate::openings::at(line, ((pair[0] + pair[1]) / 2.) as f32);
            let on_edge = edges
                .iter()
                .any(|e| crate::render::segment_distance(p, e[0], e[1]) < 1e-6);
            let inside = if on_edge {
                boundary
            } else {
                crate::geometry::inside(masks, p)
            };
            (!inside).then_some((pair[0] as f32, pair[1] as f32))
        })
        .collect()
}

pub fn boundary(item: &Item, rooms: &[Vec<Point>]) -> Vec<[Point; 2]> {
    let (Some(cave), ItemKind::TerrainRegion { rings, .. }) = (&item.cave, &item.kind) else {
        return vec![];
    };
    // Masks must be united, not XORed: a room inside a blast must stay open.
    let masks =
        crate::geometry::union_rooms(&[cave.blasts.clone(), rooms.to_vec()]).unwrap_or_default();
    edges(rings)
        .into_iter()
        .flat_map(|line| {
            outside(line, &masks, true)
                .into_iter()
                .map(move |(lo, hi)| [crate::openings::at(line, lo), crate::openings::at(line, hi)])
        })
        .collect()
}

pub fn room_cuts(level: &Level) -> Vec<Vec<Point>> {
    crate::geometry::union_rooms(
        &level
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
            .filter(|i| i.cave.is_some())
            .map(footprint)
            .collect::<Vec<_>>(),
    )
    .unwrap_or_default()
}
