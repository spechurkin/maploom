//! Roof construction shared by the editor scene, selection and exports.
use crate::document::Point;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofType {
    #[default]
    Gable,
    Hip,
    Dormer,
}
pub fn default_sun() -> f32 {
    135.
}
pub fn default_contrast() -> f32 {
    0.3
}

pub struct Facet {
    pub points: Vec<Point>,
    pub origin: Point,
    pub along: Point,
    pub down: Point,
}
impl Facet {
    pub fn uv(&self, p: Point, scale: f32) -> Point {
        let d = sub(p, self.origin);
        Point::new(dot(d, self.along) / scale, dot(d, self.down) / scale)
    }
    pub fn brightness(&self, shade: bool, sun: f32, contrast: f32) -> f32 {
        if !shade {
            return 1.;
        }
        let a = sun.to_radians();
        1. - contrast * (1. - dot(self.down, Point::new(a.cos(), a.sin()))) / 2.
    }
}
pub struct RoofGeometry {
    pub outline: Vec<Point>,
    pub facets: Vec<Facet>,
    pub ridge: Vec<Point>,
    pub hips: Vec<[Point; 2]>,
}
fn add(a: Point, b: Point) -> Point {
    Point::new(a.x + b.x, a.y + b.y)
}
fn sub(a: Point, b: Point) -> Point {
    Point::new(a.x - b.x, a.y - b.y)
}
fn mul(a: Point, s: f32) -> Point {
    Point::new(a.x * s, a.y * s)
}
fn dot(a: Point, b: Point) -> f32 {
    a.x * b.x + a.y * b.y
}

pub fn build(points: &[Point], width: f32, kind: RoofType) -> RoofGeometry {
    let mut g = RoofGeometry {
        outline: vec![],
        facets: vec![],
        ridge: points.to_vec(),
        hips: vec![],
    };
    if points.len() < 2 || width <= 0. {
        return g;
    }
    let half = width / 2.;
    let directions: Vec<_> = points
        .windows(2)
        .map(|p| mul(sub(p[1], p[0]), 1. / p[0].distance(p[1]).max(1e-7)))
        .collect();
    let normals: Vec<_> = directions.iter().map(|d| Point::new(-d.y, d.x)).collect();
    let n = points.len() - 1;
    let mut left = vec![];
    let mut right = vec![];
    for (i, p) in points.iter().enumerate() {
        let offset = if i == 0 {
            mul(normals[0], half)
        } else if i == n {
            mul(normals[n - 1], half)
        } else {
            // Intersection of adjacent offset lines, rather than overlapping rectangles.
            let v = add(normals[i - 1], normals[i]);
            mul(v, half / (1. + dot(normals[i - 1], normals[i])).max(1e-6))
        };
        left.push(add(*p, offset));
        right.push(sub(*p, offset));
    }
    if kind == RoofType::Hip {
        let start = half.min(points[0].distance(points[1]) / 2.);
        let end = half.min(points[n].distance(points[n - 1]) / 2.);
        g.ridge[0] = add(points[0], mul(directions[0], start));
        g.ridge[n] = sub(points[n], mul(directions[n - 1], end));
        for (i, down) in [(0, mul(directions[0], -1.)), (n, directions[n - 1])] {
            g.facets.push(Facet {
                points: vec![left[i], right[i], g.ridge[i]],
                origin: g.ridge[i],
                along: Point::new(-down.y, down.x),
                down,
            });
            g.hips
                .extend([[left[i], g.ridge[i]], [right[i], g.ridge[i]]]);
        }
    } else if kind == RoofType::Dormer {
        // One open end joins a parent roof; the other terminates in a pointed valley.
        let inset = half.min(points[n].distance(points[n - 1]) * 0.9);
        left[n] = add(
            sub(points[n], mul(directions[n - 1], inset)),
            mul(normals[n - 1], half),
        );
        right[n] = sub(
            sub(points[n], mul(directions[n - 1], inset)),
            mul(normals[n - 1], half),
        );
    }
    for i in 0..n {
        for (edge, sign) in [(&left, 1.), (&right, -1.)] {
            g.facets.push(Facet {
                points: vec![edge[i], edge[i + 1], g.ridge[i + 1], g.ridge[i]],
                origin: points[i],
                along: directions[i],
                down: mul(normals[i], sign),
            });
        }
    }
    g.outline.extend(left);
    if kind == RoofType::Dormer {
        g.outline.push(points[n]);
    }
    g.outline.extend(right.into_iter().rev());
    g
}

pub fn valid(points: &[Point], width: f32, kind: RoofType) -> bool {
    if points.len() < 2
        || !width.is_finite()
        || !(0.05..=512.).contains(&width)
        || points.iter().any(|p| !p.finite())
        || points.windows(2).any(|p| p[0].distance(p[1]) < 0.01)
    {
        return false;
    }
    let g = build(points, width, kind);
    let r = &g.outline;
    // Reject reversals, tight turns and self crossings that fold the eaves over each other.
    for i in 0..r.len() {
        let a = r[i];
        let b = r[(i + 1) % r.len()];
        if !a.finite() || a.distance(b) < 1e-5 {
            return false;
        }
        for j in i + 2..r.len() {
            if i == 0 && j == r.len() - 1 {
                continue;
            }
            let c = r[j];
            let d = r[(j + 1) % r.len()];
            let cross = |p: Point, q: Point, s: Point| {
                (q.x - p.x) * (s.y - p.y) - (q.y - p.y) * (s.x - p.x)
            };
            if cross(a, b, c) * cross(a, b, d) < 0. && cross(c, d, a) * cross(c, d, b) < 0. {
                return false;
            }
            if [
                crate::render::segment_distance(a, c, d),
                crate::render::segment_distance(b, c, d),
                crate::render::segment_distance(c, a, b),
                crate::render::segment_distance(d, a, b),
            ]
            .iter()
            .any(|d| *d < 1e-5)
            {
                return false;
            }
        }
    }
    crate::geometry::area(r).abs() > 1e-5
}
