//! Straight portal chords on continuous wall contours. The stored segment/t is
//! an anchor on the original contour; wall vertices are never destructively cut.
use crate::document::{Item, ItemKind, Point, WallAttachment};

pub struct PortalGeometry {
    pub bounds: [Point; 2],
    pub cuts: Vec<([Point; 2], f32, f32)>,
}

struct Contour {
    first: usize,
    points: Vec<Point>,
    distances: Vec<f64>,
    closed: bool,
}

fn contours(wall: &Item) -> Vec<Contour> {
    let rings: Vec<_> = match &wall.kind {
        ItemKind::Wall { points, .. } => vec![(points.clone(), false)],
        ItemKind::Room {
            rings,
            wall: Some(_),
            ..
        } => rings
            .iter()
            .enumerate()
            .filter(|(n, _)| !wall.wall_exclusions.contains(n))
            .map(|(_, r)| (r.clone(), true))
            .collect(),
        _ => vec![],
    };
    let mut first = 0;
    rings
        .into_iter()
        .filter_map(|(mut points, close)| {
            if points.len() < 2 {
                return None;
            }
            if close {
                points.push(points[0]);
            }
            let closed = points[0].distance(*points.last()?) < 1e-6;
            let mut distances = vec![0.];
            for pair in points.windows(2) {
                let dx = f64::from(pair[1].x) - f64::from(pair[0].x);
                let dy = f64::from(pair[1].y) - f64::from(pair[0].y);
                distances.push(distances.last().unwrap() + dx.hypot(dy));
            }
            let contour = Contour {
                first,
                points,
                distances,
                closed,
            };
            first += contour.points.len() - 1;
            Some(contour)
        })
        .collect()
}

impl Contour {
    fn total(&self) -> f64 {
        *self.distances.last().unwrap()
    }
    fn at(&self, distance: f64) -> Point {
        let s = if self.closed {
            distance.rem_euclid(self.total())
        } else {
            distance.clamp(0., self.total())
        };
        let n = self
            .distances
            .partition_point(|d| *d <= s)
            .saturating_sub(1)
            .min(self.points.len() - 2);
        let len = self.distances[n + 1] - self.distances[n];
        crate::openings::at(
            [self.points[n], self.points[n + 1]],
            if len > 1e-10 {
                ((s - self.distances[n]) / len) as f32
            } else {
                0.
            },
        )
    }
}

pub fn resolve(wall: &Item, attachment: &WallAttachment) -> Option<PortalGeometry> {
    if !attachment.length.is_finite() || attachment.length <= 0. || !attachment.t.is_finite() {
        return None;
    }
    let contour = contours(wall).into_iter().find(|c| {
        attachment.segment >= c.first && attachment.segment < c.first + c.points.len() - 1
    })?;
    let n = attachment.segment - contour.first;
    let total = contour.total();
    let width = f64::from(attachment.length);
    if total < width || total < 1e-8 {
        return None;
    }
    let center = contour.distances[n]
        + (contour.distances[n + 1] - contour.distances[n]) * f64::from(attachment.t.clamp(0., 1.));
    // Keep ordinary architectural corners intact. A smooth run may span any
    // number of tessellation edges, but a door near a square corner still fits
    // on its own side, as it did before continuous-contour placement.
    let count = contour.points.len() - 1;
    let mut breaks = vec![];
    for i in 0..count {
        if i == 0 && !contour.closed {
            continue;
        }
        let a = contour.points[(i + count - 1) % count];
        let b = contour.points[i];
        let c = contour.points[i + 1];
        let ab = (f64::from(b.x - a.x), f64::from(b.y - a.y));
        let bc = (f64::from(c.x - b.x), f64::from(c.y - b.y));
        let denom = ab.0.hypot(ab.1) * bc.0.hypot(bc.1);
        if denom > 1e-12 && (ab.0 * bc.0 + ab.1 * bc.1) / denom < std::f64::consts::FRAC_1_SQRT_2 {
            breaks.push(contour.distances[i]);
        }
    }
    let looped = contour.closed && breaks.is_empty();
    let anchor = (contour.distances[n] + contour.distances[n + 1]) / 2.;
    let (left, right) = if looped {
        (0., total)
    } else if contour.closed {
        (
            breaks
                .iter()
                .copied()
                .filter(|s| *s <= anchor)
                .next_back()
                .unwrap_or(breaks[breaks.len() - 1] - total),
            breaks
                .iter()
                .copied()
                .find(|s| *s > anchor)
                .unwrap_or(breaks[0] + total),
        )
    } else {
        (
            breaks
                .iter()
                .copied()
                .filter(|s| *s <= anchor)
                .next_back()
                .unwrap_or(0.),
            breaks
                .iter()
                .copied()
                .find(|s| *s > anchor)
                .unwrap_or(total),
        )
    };
    let available = right - left;
    if available < width {
        return None;
    }
    let interval = |h: f64| {
        let start = if looped {
            center - h
        } else {
            (center - h).clamp(left, right - 2. * h)
        };
        (start, start + 2. * h)
    };
    let chord = |h| {
        let (a, b) = interval(h);
        let [a, b] = [contour.at(a), contour.at(b)];
        (f64::from(b.x) - f64::from(a.x)).hypot(f64::from(b.y) - f64::from(a.y))
    };
    // Between these events each endpoint moves linearly on one edge. The norm
    // of their difference is convex, so the first crossing cannot be skipped.
    let mut events = vec![width / 2., available / 2.];
    for &s in &contour.distances {
        if looped {
            events.extend([
                (s - center).rem_euclid(total),
                (center - s).rem_euclid(total),
            ]);
        } else {
            for shift in [-total, 0., total] {
                let s = s + shift;
                if s >= left && s <= right {
                    events.extend([(s - center).abs(), (s - left) / 2., (right - s) / 2.]);
                }
            }
        }
    }
    events.retain(|h| *h >= width / 2. && *h <= available / 2.);
    events.sort_by(f64::total_cmp);
    let mut low = 0.;
    let mut half = None;
    for high in events {
        if chord(high) + 1e-6 >= width {
            let mut high = high;
            for _ in 0..40 {
                let mid = (low + high) / 2.;
                if chord(mid) < width {
                    low = mid;
                } else {
                    high = mid;
                }
            }
            half = Some(high);
            break;
        }
        low = high;
    }
    let (start, end) = interval(half?);
    let mut cuts = vec![];
    for (i, pair) in contour.points.windows(2).enumerate() {
        let a = contour.distances[i];
        let b = contour.distances[i + 1];
        if b - a < 1e-10 {
            continue;
        }
        for shift in if contour.closed {
            vec![-total, 0., total]
        } else {
            vec![0.]
        } {
            let lo = ((start - shift - a) / (b - a)).clamp(0., 1.);
            let hi = ((end - shift - a) / (b - a)).clamp(0., 1.);
            if hi > lo + 1e-8 {
                cuts.push(([pair[0], pair[1]], lo as f32, hi as f32));
            }
        }
    }
    Some(PortalGeometry {
        bounds: [contour.at(start), contour.at(end)],
        cuts,
    })
}

pub fn attach(
    wall: &Item,
    position: Point,
    length: f32,
) -> Option<(WallAttachment, PortalGeometry)> {
    let lines = wall.wall_segments();
    let (segment, line) = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l[0].distance(l[1]) > 1e-7)
        .min_by(|(_, a), (_, b)| {
            crate::render::segment_distance(position, a[0], a[1])
                .total_cmp(&crate::render::segment_distance(position, b[0], b[1]))
        })?;
    let len = line[0].distance(line[1]);
    let t = (((position.x - line[0].x) * (line[1].x - line[0].x)
        + (position.y - line[0].y) * (line[1].y - line[0].y))
        / (len * len))
        .clamp(0., 1.);
    let attachment = WallAttachment {
        wall: wall.id.clone(),
        segment,
        t,
        length,
    };
    let geometry = resolve(wall, &attachment)?;
    Some((attachment, geometry))
}
