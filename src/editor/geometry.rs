use crate::document::Point;
use anyhow::{ensure, Result};
use i_overlay::{
    core::{fill_rule::FillRule, overlay_rule::OverlayRule},
    float::single::SingleFloatOverlay,
};
use std::collections::BTreeMap;
type Vertex = (usize, usize);
fn trace(
    mask: &[bool],
    nx: usize,
    ny: usize,
    x: &[f32],
    y: &[f32],
    smooth: bool,
) -> Vec<Vec<Point>> {
    let filled = |a: usize, b: usize| a < nx && b < ny && mask[b * nx + a];
    let mut edges: BTreeMap<Vertex, Vec<Vertex>> = BTreeMap::new();
    let mut edge = |a, b| {
        edges.entry(a).or_default().push(b);
    };
    for row in 0..ny {
        for col in 0..nx {
            if !filled(col, row) {
                continue;
            }
            if row == 0 || !filled(col, row - 1) {
                edge((col, row), (col + 1, row));
            }
            if col + 1 == nx || !filled(col + 1, row) {
                edge((col + 1, row), (col + 1, row + 1));
            }
            if row + 1 == ny || !filled(col, row + 1) {
                edge((col + 1, row + 1), (col, row + 1));
            }
            if col == 0 || !filled(col - 1, row) {
                edge((col, row + 1), (col, row));
            }
        }
    }
    let mut rings = vec![];
    while let Some((&start, _)) = edges.first_key_value() {
        let mut current = start;
        let mut previous = (start.0 as i64 - 1, start.1 as i64);
        let mut ring = vec![];
        loop {
            ring.push(Point::new(x[current.0], y[current.1]));
            let Some(nexts) = edges.get_mut(&current) else {
                break;
            };
            let incoming = (current.0 as i64 - previous.0, current.1 as i64 - previous.1);
            let i = nexts
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| {
                    let outgoing = (v.0 as i64 - current.0 as i64, v.1 as i64 - current.1 as i64);
                    let cross = incoming.0 * outgoing.1 - incoming.1 * outgoing.0;
                    let dot = incoming.0 * outgoing.0 + incoming.1 * outgoing.1;
                    if cross > 0 {
                        3
                    } else if dot > 0 {
                        2
                    } else if cross < 0 {
                        1
                    } else {
                        0
                    }
                })
                .map(|(i, _)| i)
                .unwrap();
            let next = nexts.swap_remove(i);
            if nexts.is_empty() {
                edges.remove(&current);
            }
            previous = (current.0 as i64, current.1 as i64);
            current = next;
            if current == start {
                break;
            }
            if ring.len() > mask.len() * 4 {
                break;
            }
        }
        if ring.len() >= 3 {
            let original = ring.clone();
            ring = (0..original.len())
                .filter_map(|i| {
                    let a = original[(i + original.len() - 1) % original.len()];
                    let b = original[i];
                    let c = original[(i + 1) % original.len()];
                    (((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)).abs() > 0.00001)
                        .then_some(b)
                })
                .collect();
            if smooth {
                for _ in 0..2 {
                    let mut next = vec![];
                    for i in 0..ring.len() {
                        let a = ring[i];
                        let b = ring[(i + 1) % ring.len()];
                        next.push(Point::new(a.x * 0.75 + b.x * 0.25, a.y * 0.75 + b.y * 0.25));
                        next.push(Point::new(a.x * 0.25 + b.x * 0.75, a.y * 0.25 + b.y * 0.75));
                    }
                    ring = next;
                }
            }
            if ring.len() >= 3 {
                rings.push(ring);
            }
        }
    }
    rings
}
pub fn inside(rings: &[Vec<Point>], p: Point) -> bool {
    rings
        .iter()
        .filter(|r| crate::render::contains_polygon(r, p))
        .count()
        % 2
        == 1
}
pub fn area(ring: &[Point]) -> f32 {
    let origin = ring.first().copied().unwrap_or_default();
    (0..ring.len())
        .map(|i| {
            let a = ring[i];
            let b = ring[(i + 1) % ring.len()];
            (a.x as f64 - origin.x as f64) * (b.y as f64 - origin.y as f64)
                - (b.x as f64 - origin.x as f64) * (a.y as f64 - origin.y as f64)
        })
        .sum::<f64>() as f32
        / 2.
}
/// Floor cutouts must be simple, disjoint and strictly inside the outer contour.
pub fn valid_floor_cutouts(outer: &[Point], holes: &[Vec<Point>]) -> bool {
    let rings: Vec<&[Point]> = std::iter::once(outer)
        .chain(holes.iter().map(Vec::as_slice))
        .collect();
    if rings
        .iter()
        .any(|r| r.len() < 3 || r.iter().any(|p| !p.finite()) || area(r).abs() < 1e-6)
    {
        return false;
    }
    let intersects = |a: Point, b: Point, c: Point, d: Point| {
        let cross = |p: Point, q: Point, r: Point| {
            (q.x - p.x) as f64 * (r.y - p.y) as f64 - (q.y - p.y) as f64 * (r.x - p.x) as f64
        };
        [
            crate::render::segment_distance(a, c, d),
            crate::render::segment_distance(b, c, d),
            crate::render::segment_distance(c, a, b),
            crate::render::segment_distance(d, a, b),
        ]
            .iter()
            .any(|n| *n < 1e-6)
            || (cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0.)
    };
    for (n, ring) in rings.iter().enumerate() {
        for i in 0..ring.len() {
            if ring[i].distance(ring[(i + 1) % ring.len()]) < 1e-6 {
                return false;
            }
            for j in i + 1..ring.len() {
                if j == i + 1 || (i == 0 && j == ring.len() - 1) {
                    continue;
                }
                if intersects(
                    ring[i],
                    ring[(i + 1) % ring.len()],
                    ring[j],
                    ring[(j + 1) % ring.len()],
                ) {
                    return false;
                }
            }
        }
        if n > 0 && !crate::render::contains_polygon(outer, ring[0]) {
            return false;
        }
        for other in rings.iter().skip(n + 1) {
            if ring.iter().enumerate().any(|(i, a)| {
                other.iter().enumerate().any(|(j, b)| {
                    intersects(
                        *a,
                        ring[(i + 1) % ring.len()],
                        *b,
                        other[(j + 1) % other.len()],
                    )
                })
            }) {
                return false;
            }
            if n > 0
                && (crate::render::contains_polygon(ring, other[0])
                || crate::render::contains_polygon(other, ring[0]))
            {
                return false;
            }
        }
    }
    true
}
pub fn union_rooms(regions: &[Vec<Vec<Point>>]) -> Result<Vec<Vec<Point>>> {
    ensure!(
        regions.iter().flatten().map(Vec::len).sum::<usize>() <= 4 * 1024 * 1024,
        "Room geometry exceeds vertex budget"
    );
    ensure!(
        regions.iter().flatten().flatten().all(|p| p.finite()),
        "Invalid room coordinates"
    );
    // Partitioning by vertex x/y values only works for axis-aligned rooms.
    // Overlay splits actual edge intersections, preserving diagonal and curved
    // (polygonal) contours. Even-odd applies within each room; union between
    // rooms must be sequential so their overlapping floors do not cancel out.
    let mut merged: Vec<Vec<[f64; 2]>> = vec![];
    for region in regions {
        let contours: Vec<Vec<_>> = region
            .iter()
            .filter(|r| r.len() >= 3)
            .map(|r| r.iter().map(|p| [p.x as f64, p.y as f64]).collect())
            .collect();
        merged = merged
            .overlay(&contours, OverlayRule::Union, FillRule::EvenOdd)
            .into_iter()
            .flatten()
            .collect();
    }
    // Overlay outputs positive outer contours and negative holes, matching the
    // room renderer, triangulation and VTT export conventions.
    Ok(merged
        .into_iter()
        .map(|r| {
            r.into_iter()
                .map(|p| Point::new(p[0] as f32, p[1] as f32))
                .collect()
        })
        .collect())
}
pub fn touches(a: &[Vec<Point>], b: &[Vec<Point>]) -> bool {
    if a.iter().flatten().any(|p| inside(b, *p)) || b.iter().flatten().any(|p| inside(a, *p)) {
        return true;
    }
    for ar in a {
        for br in b {
            for i in 0..ar.len() {
                for j in 0..br.len() {
                    let (a1, a2, b1, b2) =
                        (ar[i], ar[(i + 1) % ar.len()], br[j], br[(j + 1) % br.len()]);
                    let cross = |p: Point, q: Point, r: Point| {
                        (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
                    };
                    if cross(a1, a2, b1) * cross(a1, a2, b2) <= 0.
                        && cross(b1, b2, a1) * cross(b1, b2, a2) <= 0.
                        && a1.x.min(a2.x) <= b1.x.max(b2.x) + 0.0001
                        && b1.x.min(b2.x) <= a1.x.max(a2.x) + 0.0001
                        && a1.y.min(a2.y) <= b1.y.max(b2.y) + 0.0001
                        && b1.y.min(b2.y) <= a1.y.max(a2.y) + 0.0001
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}
pub fn terrain_contours(stamps: &[(Point, f32)]) -> Vec<Vec<Point>> {
    if stamps.is_empty() {
        return vec![];
    }
    let minx = stamps
        .iter()
        .map(|(p, r)| p.x - r)
        .fold(f32::INFINITY, f32::min);
    let miny = stamps
        .iter()
        .map(|(p, r)| p.y - r)
        .fold(f32::INFINITY, f32::min);
    let maxx = stamps
        .iter()
        .map(|(p, r)| p.x + r)
        .fold(f32::NEG_INFINITY, f32::max);
    let maxy = stamps
        .iter()
        .map(|(p, r)| p.y + r)
        .fold(f32::NEG_INFINITY, f32::max);
    let step = 0.125f32
        .max((maxx - minx) / 2046.)
        .max((maxy - miny) / 2046.);
    let nx = ((maxx - minx) / step).ceil() as usize + 2;
    let ny = ((maxy - miny) / step).ceil() as usize + 2;
    let xs: Vec<_> = (0..=nx).map(|i| minx - step + i as f32 * step).collect();
    let ys: Vec<_> = (0..=ny).map(|i| miny - step + i as f32 * step).collect();
    let mut mask = vec![false; nx * ny];
    for (p, r) in stamps {
        let x0 = ((p.x - r - xs[0]) / step).floor().max(0.) as usize;
        let x1 = (((p.x + r - xs[0]) / step).ceil() as usize).min(nx);
        let y0 = ((p.y - r - ys[0]) / step).floor().max(0.) as usize;
        let y1 = (((p.y + r - ys[0]) / step).ceil() as usize).min(ny);
        for row in y0..y1 {
            for col in x0..x1 {
                if Point::new((xs[col] + xs[col + 1]) / 2., (ys[row] + ys[row + 1]) / 2.)
                    .distance(*p)
                    <= *r
                {
                    mask[row * nx + col] = true;
                }
            }
        }
    }
    trace(&mask, nx, ny, &xs, &ys, true)
}
pub fn polygon_with_holes(outer: &[Point], holes: &[Vec<Point>]) -> Vec<Point> {
    let mut polygon = outer.to_vec();
    for hole in holes {
        let h = hole
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.x.total_cmp(&b.x))
            .map(|(i, _)| i)
            .unwrap_or(0);
        let hp = hole[h];
        let mut closest = None;
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            if (a.y > hp.y) != (b.y > hp.y) {
                let x = a.x + (hp.y - a.y) * (b.x - a.x) / (b.y - a.y);
                if x >= hp.x && closest.is_none_or(|(_, best)| x < best) {
                    closest = Some((i, x));
                }
            }
        }
        if let Some((i, x)) = closest {
            let bridge = Point::new(x, hp.y);
            let mut next = polygon[..=i].to_vec();
            next.push(bridge);
            for j in 0..=hole.len() {
                next.push(hole[(h + j) % hole.len()]);
            }
            next.push(bridge);
            next.extend_from_slice(&polygon[i + 1..]);
            polygon = next;
        }
    }
    polygon
}
pub fn fill_polygons(rings: &[Vec<Point>]) -> Vec<Vec<Point>> {
    let holes: Vec<_> = rings.iter().filter(|r| area(r) < 0.).cloned().collect();
    rings
        .iter()
        .filter(|r| area(r) > 0.)
        .map(|outer| {
            polygon_with_holes(
                outer,
                &holes
                    .iter()
                    .filter(|h| crate::render::contains_polygon(outer, h[0]))
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}
