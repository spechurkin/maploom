//! Textured paths share their interpolated ribbon across rendering, selection and exports.
use crate::document::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    None,
    Fade,
    Shrink,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathStyle {
    pub smoothness: f32,
    pub transition_in: Transition,
    pub transition_out: Transition,
    pub blocks_light: bool,
}
impl PathStyle {
    pub fn valid(&self) -> bool {
        self.smoothness.is_finite() && (0.0..=1.).contains(&self.smoothness)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PathVertex {
    pub position: Point,
    pub uv: Point,
    pub alpha: f32,
}

/// Imported sampled curves may carry their original width/alpha at each point.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathProfile {
    pub widths: Vec<f32>,
    pub alpha: Vec<f32>,
}
impl PathProfile {
    pub fn valid(&self, count: usize) -> bool {
        self.widths.len() == count
            && self.alpha.len() == count
            && count <= 10000
            && self
            .widths
            .iter()
            .chain(&self.alpha)
            .all(|v| v.is_finite() && (0.0..=1.).contains(v))
    }
}

pub fn sample(points: &[Point], smoothness: f32) -> Vec<Point> {
    let mut ps = points.to_vec();
    ps.dedup_by(|a, b| a.distance(*b) <= 1e-5);
    if ps.len() < 2 {
        return ps;
    }
    let s = smoothness.clamp(0., 1.);
    let mut out = vec![ps[0]];
    for n in 0..ps.len() - 1 {
        let (a, b, prev, next) = (
            ps[n],
            ps[n + 1],
            ps[n.saturating_sub(1)],
            ps[(n + 2).min(ps.len() - 1)],
        );
        let steps = (a.distance(b) / 0.15).ceil().clamp(8., 64.) as usize;
        for k in 1..=steps {
            let t = k as f32 / steps as f32;
            let t2 = t * t;
            let t3 = t2 * t;
            let coordinate = |a: f32, b: f32, prev: f32, next: f32| {
                let curve = (2. * t3 - 3. * t2 + 1.) * a
                    + (t3 - 2. * t2 + t) * (b - prev) / 2.
                    + (-2. * t3 + 3. * t2) * b
                    + (t3 - t2) * (next - a) / 2.;
                (a + (b - a) * t) * (1. - s) + curve * s
            };
            out.push(Point::new(
                coordinate(a.x, b.x, prev.x, next.x),
                coordinate(a.y, b.y, prev.y, next.y),
            ));
        }
    }
    out
}

pub fn ribbon(points: &[Point], width: f32, scale: f32, style: &PathStyle) -> Vec<PathVertex> {
    ribbon_profile(points, width, scale, style, None)
}
fn ribbon_profile(
    points: &[Point],
    width: f32,
    scale: f32,
    style: &PathStyle,
    profile: Option<&PathProfile>,
) -> Vec<PathVertex> {
    let profile = profile.filter(|p| p.valid(points.len()));
    let mut ps = if profile.is_some() {
        points.to_vec()
    } else {
        sample(points, style.smoothness)
    };
    if ps.len() < 2 {
        return vec![];
    }
    let total: f32 = ps.windows(2).map(|p| p[0].distance(p[1])).sum();
    let transition = width.min(total / 2.);
    let mut cuts = vec![];
    for t in [0.25, 0.5, 0.75, 1.] {
        if style.transition_in != Transition::None {
            cuts.push(transition * t);
        }
        if style.transition_out != Transition::None {
            cuts.push(total - transition * t);
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.dedup();
    if profile.is_none() && !cuts.is_empty() {
        let mut refined = vec![ps[0]];
        let mut travelled = 0.;
        for line in ps.windows(2) {
            let len = line[0].distance(line[1]);
            for cut in &cuts {
                if *cut > travelled + 1e-6 && *cut < travelled + len - 1e-6 {
                    refined.push(crate::openings::at(
                        [line[0], line[1]],
                        (*cut - travelled) / len,
                    ));
                }
            }
            refined.push(line[1]);
            travelled += len;
        }
        ps = refined;
    }
    let lengths: Vec<_> = ps.windows(2).map(|p| p[0].distance(p[1])).collect();
    let normal = |a: Point, b: Point| {
        let d = a.distance(b).max(1e-7);
        Point::new(-(b.y - a.y) / d, (b.x - a.x) / d)
    };
    let mut travelled = 0.;
    let mut out = vec![];
    for (n, p) in ps.iter().enumerate() {
        if n > 0 {
            travelled += lengths[n - 1];
        }
        let incoming = normal(ps[n.saturating_sub(1)], if n > 0 { *p } else { ps[1] });
        let outgoing = if n == ps.len() - 1 {
            incoming
        } else {
            normal(*p, ps[n + 1])
        };
        let (sx, sy) = (incoming.x + outgoing.x, incoming.y + outgoing.y);
        let len = sx.hypot(sy);
        let avg = if len > 1e-5 {
            Point::new(sx / len, sy / len)
        } else {
            outgoing
        };
        let divisor = (avg.x * outgoing.x + avg.y * outgoing.y).max(0.5);
        let start = (travelled / transition.max(1e-7)).clamp(0., 1.);
        let end = ((total - travelled) / transition.max(1e-7)).clamp(0., 1.);
        let factor = profile.map(|p| p.widths[n]).unwrap_or_else(|| if style.transition_in == Transition::Shrink { start } else { 1. } * if style.transition_out == Transition::Shrink { end } else { 1. });
        let alpha = profile.map(|p| p.alpha[n]).unwrap_or_else(|| if style.transition_in == Transition::Fade { start } else { 1. } * if style.transition_out == Transition::Fade { end } else { 1. });
        for (row, sign) in [1., -1.].into_iter().enumerate() {
            out.push(PathVertex {
                position: Point::new(
                    p.x + avg.x / divisor * width / 2. * sign * factor,
                    p.y + avg.y / divisor * width / 2. * sign * factor,
                ),
                uv: Point::new(travelled / scale.max(0.01), row as f32),
                alpha,
            });
        }
    }
    out
}

pub fn item_ribbon(item: &crate::document::Item) -> Vec<PathVertex> {
    if let crate::document::ItemKind::Path { points, width, .. } = &item.kind {
        ribbon_profile(
            points,
            *width,
            item.texture_scale,
            &item.path_style.clone().unwrap_or_default(),
            item.path_profile.as_ref(),
        )
    } else {
        vec![]
    }
}
