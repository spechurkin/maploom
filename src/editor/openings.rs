//! Attached portals cut their own contour edges. Imported, unattached portals
//! retain the legacy collinear matching rule.
use crate::document::{ItemKind, Level, Point};

#[derive(Clone)]
pub struct Openings {
    loose: Vec<[Point; 2]>,
    attached: Vec<(String, [Point; 2], f32, f32)>,
    cave_cuts: Vec<Vec<Point>>,
    rooms: Vec<String>,
}

impl Openings {
    pub fn new(level: &Level) -> Self {
        let mut result = Self {
            loose: vec![],
            attached: vec![],
            cave_cuts: crate::caves::room_cuts(level),
            rooms: vec![],
        };
        let visible: Vec<_> = level
            .layers
            .iter()
            .filter(|l| l.visible)
            .flat_map(|l| &l.items)
            .collect();
        for item in &visible {
            if matches!(item.kind, ItemKind::Room { .. }) {
                result.rooms.push(item.id.clone());
            }
            if let ItemKind::Portal { bounds, .. } = &item.kind {
                if let Some(a) = &item.wall_attachment {
                    if let Some(geometry) = visible
                        .iter()
                        .find(|wall| wall.id == a.wall)
                        .and_then(|wall| crate::portals::resolve(wall, a))
                    {
                        result.attached.extend(
                            geometry
                                .cuts
                                .into_iter()
                                .map(|(line, lo, hi)| (a.wall.clone(), line, lo, hi)),
                        );
                    }
                } else {
                    result.loose.push(*bounds);
                }
            }
        }
        result
    }

    pub fn for_wall(&self, wall: &str) -> Self {
        Self {
            loose: self.loose.clone(),
            attached: self
                .attached
                .iter()
                .filter(|(id, ..)| id == wall)
                .cloned()
                .collect(),
            cave_cuts: if self.rooms.iter().any(|id| id == wall) {
                self.cave_cuts.clone()
            } else {
                vec![]
            },
            rooms: vec![],
        }
    }

    /// Remaining wall intervals in segment coordinates. Overlapping cuts are united.
    pub fn spans(&self, line: [Point; 2]) -> Vec<(f32, f32)> {
        let [a, b] = line;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len2 = dx * dx + dy * dy;
        if len2 < 1e-8 {
            return vec![];
        }
        let mut cuts = vec![];
        for (_, edge, lo, hi) in &self.attached {
            if edge[0].distance(a) < 1e-6 && edge[1].distance(b) < 1e-6 {
                cuts.push((*lo, *hi));
            } else if edge[1].distance(a) < 1e-6 && edge[0].distance(b) < 1e-6 {
                cuts.push((1. - hi, 1. - lo));
            }
        }
        for bounds in &self.loose {
            let distance = |p: Point| ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len2.sqrt();
            if bounds.iter().all(|p| distance(*p) < 0.025) {
                let t = |p: Point| ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
                let lo = t(bounds[0]).min(t(bounds[1])).max(0.);
                let hi = t(bounds[0]).max(t(bounds[1])).min(1.);
                if hi > lo {
                    cuts.push((lo, hi));
                }
            }
        }
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cursor = 0.;
        let mut spans = vec![];
        for (lo, hi) in cuts {
            if lo > cursor {
                spans.push((cursor, lo));
            }
            cursor = cursor.max(hi);
        }
        if cursor < 1. {
            spans.push((cursor, 1.));
        }
        let outside = crate::caves::outside(line, &self.cave_cuts, false);
        spans
            .into_iter()
            .flat_map(|(lo, hi)| {
                outside.iter().filter_map(move |(a, b)| {
                    let start = lo.max(*a);
                    let end = hi.min(*b);
                    (end > start).then_some((start, end))
                })
            })
            .collect()
    }
}

pub fn at([a, b]: [Point; 2], t: f32) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
