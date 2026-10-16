//! Editor operations shared by the desktop bridge and engine tests.
use crate::{document::*, geometry, render};
use anyhow::{Context, Result, ensure};
use std::collections::{BTreeMap, HashSet};

/// Move the map origin together with all content, including locked/hidden layers.
/// Cropping clips the view; off-map geometry is retained for undo and re-expansion.
pub fn resize_map(
    project: &mut Project,
    top: i32,
    right: i32,
    bottom: i32,
    left: i32,
) -> Result<()> {
    let width = i64::from(project.width) + i64::from(left) + i64::from(right);
    let height = i64::from(project.height) + i64::from(top) + i64::from(bottom);
    ensure!(
        (1..=512).contains(&width) && (1..=512).contains(&height),
        "Map dimensions must be 1–512 grid cells"
    );
    // Validate on a copy so a rejected operation cannot partly change the document.
    let mut next = project.clone();
    next.width = width as u32;
    next.height = height as u32;
    for item in next
        .all_levels_mut()
        .flat_map(|l| &mut l.layers)
        .flat_map(|l| &mut l.items)
    {
        item.translate(Point::new(left as f32, top as f32));
    }
    next.sync_portals();
    next.sync_active_variation();
    next.validate()?;
    *project = next;
    Ok(())
}

/// Preserve door/window attachments when contour editing changes segment indices.
pub fn reconcile_geometry(before: &Project, after: &mut Project) {
    let old: BTreeMap<_, _> = before
        .levels
        .iter()
        .flat_map(|l| &l.layers)
        .flat_map(|l| &l.items)
        .map(|i| (i.id.clone(), i))
        .collect();
    let changed: BTreeMap<_, _> = after
        .levels
        .iter()
        .flat_map(|l| &l.layers)
        .flat_map(|l| &l.items)
        .filter_map(|i| {
            let previous = old.get(&i.id)?;
            let a = previous.wall_segments();
            let b = i.wall_segments();
            if a == b {
                return None;
            }
            let same_topology = previous.wall_exclusions == i.wall_exclusions
                && match (&previous.kind, &i.kind) {
                    (ItemKind::Room { rings: a, .. }, ItemKind::Room { rings: b, .. }) => {
                        a.iter().map(Vec::len).eq(b.iter().map(Vec::len))
                    }
                    (ItemKind::Wall { points: a, .. }, ItemKind::Wall { points: b, .. }) => {
                        a.len() == b.len()
                    }
                    _ => false,
                };
            Some((i.id.clone(), (a, b, same_topology)))
        })
        .collect();
    for item in after
        .levels
        .iter_mut()
        .flat_map(|l| &mut l.layers)
        .flat_map(|l| &mut l.items)
    {
        if let Some(previous) = old.get(&item.id) {
            if item.path_profile == previous.path_profile
                && (item.path_style != previous.path_style
                    || matches!((&previous.kind, &item.kind), (ItemKind::Path { points: a, .. }, ItemKind::Path { points: b, .. }) if a.len() != b.len()))
            {
                item.path_profile = None;
            }
        }
        let Some(attachment) = &mut item.wall_attachment else {
            continue;
        };
        let Some((a, b, same_topology)) = changed.get(&attachment.wall) else {
            continue;
        };
        let Some(previous) = a.get(attachment.segment) else {
            item.wall_attachment = None;
            continue;
        };
        let center = Point::new(
            previous[0].x + (previous[1].x - previous[0].x) * attachment.t,
            previous[0].y + (previous[1].y - previous[0].y) * attachment.t,
        );
        let target = if *same_topology {
            b.get(attachment.segment)
                .map(|line| (attachment.segment, line))
        } else {
            b.iter()
                .enumerate()
                .filter(|(_, line)| line[0].distance(line[1]) > 1e-7)
                .min_by(|(_, x), (_, y)| {
                    render::segment_distance(center, x[0], x[1])
                        .total_cmp(&render::segment_distance(center, y[0], y[1]))
                })
                .filter(|(_, line)| render::segment_distance(center, line[0], line[1]) < 0.3)
        };
        if let Some((n, line)) = target.filter(|(_, line)| line[0].distance(line[1]) > 1e-7) {
            let length = line[0].distance(line[1]);
            attachment.segment = n;
            if !same_topology {
                attachment.t = ((center.x - line[0].x) * (line[1].x - line[0].x)
                    + (center.y - line[0].y) * (line[1].y - line[0].y))
                    / (length * length);
            }
            attachment.t = attachment.t.clamp(0., 1.);
        } else {
            item.wall_attachment = None;
        }
    }
}

pub fn protect_locked(before: &Project, after: &Project) -> Result<()> {
    for level in &before.levels {
        for layer in &level.layers {
            let next = after
                .levels
                .iter()
                .flat_map(|l| &l.layers)
                .find(|l| l.id == layer.id);
            if layer.locked {
                ensure!(
                    next.is_some_and(|l| l.items == layer.items),
                    "Layer is locked"
                );
            }
            for item in layer.items.iter().filter(|i| i.locked) {
                let next = after
                    .levels
                    .iter()
                    .flat_map(|l| &l.layers)
                    .flat_map(|l| &l.items)
                    .find(|i| i.id == item.id);
                let mut unlocked = item.clone();
                unlocked.locked = false;
                ensure!(
                    next.is_some_and(|i| i == item || i == &unlocked),
                    "Item is locked"
                );
            }
        }
    }
    Ok(())
}

pub fn move_items(project: &mut Project, level: usize, ids: &HashSet<String>, delta: Point) {
    let layers = &mut project.levels[level].layers;
    let movable: HashSet<_> = layers
        .iter()
        .filter(|l| !l.locked)
        .flat_map(|l| &l.items)
        .filter(|i| ids.contains(&i.id) && !i.locked)
        .map(|i| i.id.clone())
        .collect();
    for item in layers
        .iter_mut()
        .filter(|l| !l.locked)
        .flat_map(|l| &mut l.items)
        .filter(|i| movable.contains(&i.id))
    {
        if item
            .wall_attachment
            .as_ref()
            .is_some_and(|a| movable.contains(&a.wall))
        {
            continue;
        }
        item.translate(delta);
    }
    let walls: BTreeMap<_, _> = layers
        .iter()
        .flat_map(|l| &l.items)
        .map(|i| (i.id.clone(), i.clone()))
        .collect();
    for item in layers
        .iter_mut()
        .flat_map(|l| &mut l.items)
        .filter(|i| movable.contains(&i.id))
    {
        if let (Some(a), ItemKind::Portal { bounds, .. }) = (&mut item.wall_attachment, &item.kind)
        {
            if movable.contains(&a.wall) {
                continue;
            }
            if let Some(wall) = walls.get(&a.wall) {
                let center = Point::new(
                    (bounds[0].x + bounds[1].x) / 2.,
                    (bounds[0].y + bounds[1].y) / 2.,
                );
                if let Some((next, _)) = crate::portals::attach(wall, center, a.length) {
                    *a = next;
                }
            }
        }
    }
    project.sync_portals();
}

pub fn delete_items(project: &mut Project, level: usize, ids: &HashSet<String>) {
    let layers = &mut project.levels[level].layers;
    let removed: HashSet<_> = layers
        .iter()
        .filter(|l| !l.locked)
        .flat_map(|l| &l.items)
        .filter(|i| ids.contains(&i.id) && !i.locked)
        .map(|i| i.id.clone())
        .collect();
    for layer in layers.iter_mut().filter(|l| !l.locked) {
        layer.items.retain(|i| {
            !removed.contains(&i.id)
                && (i.locked
                    || !i
                        .wall_attachment
                        .as_ref()
                        .is_some_and(|a| removed.contains(&a.wall)))
        });
    }
    project.sync_portals();
}

pub fn duplicate_items(project: &mut Project, level: usize, ids: &HashSet<String>) -> Vec<String> {
    let layers = &mut project.levels[level].layers;
    let mapping: BTreeMap<_, _> = layers
        .iter()
        .filter(|l| !l.locked)
        .flat_map(|l| &l.items)
        .filter(|i| ids.contains(&i.id) && !i.locked)
        .map(|i| (i.id.clone(), id()))
        .collect();
    for layer in layers.iter_mut().filter(|l| !l.locked) {
        let copies: Vec<_> = layer
            .items
            .iter()
            .filter_map(|i| {
                mapping.get(&i.id).map(|new_id| {
                    let mut item = i.clone();
                    item.id = new_id.clone();
                    item.translate(Point::new(0.5, 0.5));
                    if let Some(a) = &mut item.wall_attachment {
                        if let Some(wall) = mapping.get(&a.wall) {
                            a.wall = wall.clone();
                        }
                    }
                    item
                })
            })
            .collect();
        layer.items.extend(copies);
    }
    project.sync_portals();
    mapping.into_values().collect()
}

pub fn add_room(project: &mut Project, level: usize, layer: usize, mut room: Item) -> Result<()> {
    let target = project
        .levels
        .get(level)
        .and_then(|l| l.layers.get(layer))
        .context("Invalid layer")?;
    ensure!(!target.locked && target.visible, "Layer is not editable");
    let ItemKind::Room { rings, .. } = &room.kind else {
        anyhow::bail!("Expected room");
    };
    let mergeable =
        |i: &Item| i.wall_exclusions.is_empty() && matches!(&i.kind, ItemKind::Room { .. });
    if !mergeable(&room) {
        project.levels[level].layers[layer].items.push(room);
        return Ok(());
    }
    let mut regions = vec![rings.clone()];
    let mut removed = HashSet::new();
    loop {
        let old_count = removed.len();
        for item in &target.items {
            if item.locked || !mergeable(item) || removed.contains(&item.id) {
                continue;
            }
            if let ItemKind::Room { rings, .. } = &item.kind {
                if regions.iter().any(|r| geometry::touches(r, rings)) {
                    regions.push(rings.clone());
                    removed.insert(item.id.clone());
                }
            }
        }
        if removed.len() == old_count {
            break;
        }
    }
    if let ItemKind::Room { rings, .. } = &mut room.kind {
        if regions.len() > 1 {
            *rings = geometry::union_rooms(&regions)?;
        }
    }
    let lines = room.wall_segments();
    let original_walls: BTreeMap<_, _> = project.levels[level]
        .layers
        .iter()
        .flat_map(|l| &l.items)
        .filter(|item| removed.contains(&item.id))
        .map(|item| (item.id.clone(), item.wall_segments()))
        .collect();
    // A portal may live in any layer. Keep exterior doors attached and remove
    // doors on walls that become interior to the merged room.
    for target in &mut project.levels[level].layers {
        target.items.retain_mut(|item| {
            if removed.contains(&item.id) {
                return false;
            }
            if let Some(a) = &mut item.wall_attachment {
                if removed.contains(&a.wall) {
                    if let ItemKind::Portal { bounds, .. } = &item.kind {
                        let center = original_walls
                            .get(&a.wall)
                            .and_then(|lines| lines.get(a.segment))
                            .map(|line| crate::openings::at(*line, a.t))
                            .unwrap_or(Point::new(
                                (bounds[0].x + bounds[1].x) / 2.,
                                (bounds[0].y + bounds[1].y) / 2.,
                            ));
                        if let Some((segment, line)) =
                            lines.iter().enumerate().min_by(|(_, a), (_, b)| {
                                render::segment_distance(center, a[0], a[1])
                                    .total_cmp(&render::segment_distance(center, b[0], b[1]))
                            })
                        {
                            let len = line[0].distance(line[1]);
                            if render::segment_distance(center, line[0], line[1]) > 0.3
                                || len < 1e-7
                            {
                                return false;
                            }
                            a.wall = room.id.clone();
                            a.segment = segment;
                            a.t = (((center.x - line[0].x) * (line[1].x - line[0].x)
                                + (center.y - line[0].y) * (line[1].y - line[0].y))
                                / (len * len))
                                .clamp(0., 1.);
                        } else {
                            return false;
                        }
                    }
                }
            }
            true
        });
    }
    project.levels[level].layers[layer].items.push(room);
    project.sync_portals();
    Ok(())
}
pub fn attach_portal(
    project: &Project,
    level: usize,
    position: Point,
    length: f32,
) -> Result<(WallAttachment, [Point; 2])> {
    let cave_mask = crate::caves::room_cuts(project.levels.get(level).context("Invalid level")?);
    let (attachment, geometry, distance) = project
        .levels
        .get(level)
        .context("Invalid level")?
        .layers
        .iter()
        .filter(|l| l.visible)
        .flat_map(|l| &l.items)
        .filter_map(|i| {
            let (attachment, geometry) = crate::portals::attach(i, position, length)?;
            if matches!(i.kind, ItemKind::Room { .. })
                && geometry.cuts.iter().any(|(line, lo, hi)| {
                    let section = [
                        crate::openings::at(*line, *lo),
                        crate::openings::at(*line, *hi),
                    ];
                    crate::caves::outside(section, &cave_mask, false) != vec![(0., 1.)]
                })
            {
                return None;
            }
            let line = i.wall_segments()[attachment.segment];
            let distance = render::segment_distance(position, line[0], line[1]);
            Some((attachment, geometry, distance))
        })
        .min_by(|(_, _, a), (_, _, b)| a.total_cmp(b))
        .context("Place the door near a wall")?;
    ensure!(distance < 1., "Place the door near a wall");
    Ok((attachment, geometry.bounds))
}

pub fn portal_item(
    project: &Project,
    level: usize,
    position: Point,
    meta: &AssetMeta,
    color: Color,
) -> Result<Item> {
    let (attachment, bounds) = attach_portal(project, level, position, meta.size.x)?;
    let mut item = Item::new(
        &meta.name,
        ItemKind::Portal {
            bounds,
            closed: meta.portal_type() == PortalType::Door,
            color,
        },
    );
    item.portal_type = Some(meta.portal_type());
    item.texture = Some(meta.id.clone());
    item.wall_attachment = Some(attachment);
    Ok(item)
}
