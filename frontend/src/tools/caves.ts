import {type Asset, type CaveStyle, type Item, type Layer, type Level, newItem, type Point} from "../map/model";
import {terrainContours, terrainRegionsTouch} from "./terrain";

export function caveStyles(assets: Asset[]) {
    return assets.filter(a => a.category === "caves" && /\/floor\.[^/]+$/i.test(a.meta.id)).flatMap(floor => {
        const stem = floor.meta.id.replace(/\/floor\.[^/]+$/i, "");
        const wall = assets.find(a => a.category === "caves" && a.meta.id.replace(/\/wall\.[^/]+$/i, "") === stem);
        return wall ? [{
            id: stem, name: stem.split("/").at(-1)!, floor, wall,
            colorable: floor.meta.color_mode === "tint" || floor.meta.tags.includes("colorable")
        }] : [];
    });
}

export const caveFootprint = (item: Item): Point[][] => item.cave && item.kind.type === "terrain_region"
    ? terrainContours([], 1, {existing: item.kind.rings, subtract: [item.cave.blasts], smoothing: 0}) : [];
export const caveOpenedAreas = (item: Item): Point[][] => item.cave && item.kind.type === "terrain_region" && item.cave.blasts.length
    ? terrainContours([], 1, {existing: item.kind.rings, subtract: [caveFootprint(item)], smoothing: 0}) : [];

// A brush stroke is one transaction. Protected touching caves reject the whole
// stroke, preserving their geometry rather than partly merging a connected body.
export function paintCave(level: Level, target: Layer, trail: Point[], radius: number,
                          mode: "dig" | "blast", style: CaveStyle, material: ReturnType<typeof caveStyles>[number] | undefined,
                          name: string): Item | null | undefined {
    if (target.locked || !target.visible) return null;
    const fresh = terrainContours(trail, radius, {smoothing: 0.22});
    if (!fresh.length) return;
    const candidates = level.layers.filter(l => l.visible).flatMap(layer => layer.items
        .filter(item => item.cave && item.kind.type === "terrain_region").map(item => ({layer, item})));
    const groups = [fresh], joined: typeof candidates = [];
    let changed = true;
    while (changed) {
        changed = false;
        for (const entry of candidates) {
            if (joined.includes(entry) || entry.item.kind.type !== "terrain_region") continue;
            if (!(mode === "blast" ? [fresh] : groups).some(g => terrainRegionsTouch(g, entry.item.kind.type === "terrain_region" ? entry.item.kind.rings : []))) continue;
            if (entry.layer.locked || entry.item.locked) return null;
            joined.push(entry);
            groups.push(entry.item.kind.rings);
            changed = true;
        }
        if (mode === "blast") break;
    }
    if (mode === "blast") {
        for (const {item} of joined) item.cave!.blasts = terrainContours([], radius,
            {regions: [item.cave!.blasts, fresh], smoothing: 0});
        return joined[0]?.item;
    }
    if (!material) return;
    const rings = terrainContours([], radius, {regions: groups, smoothing: 0});
    const visible = terrainContours([], radius, {
        regions: [fresh, ...joined.map(e => caveFootprint(e.item))],
        smoothing: 0
    });
    const blasts = terrainContours([], radius, {existing: rings, subtract: [visible], smoothing: 0});
    const item = joined[0]?.item ?? newItem(name, {type: "terrain_region", rings, color: [255, 255, 255, 255]});
    item.kind = {type: "terrain_region", rings, color: [255, 255, 255, 255]};
    item.cave = {...structuredClone(style), colorable: material.colorable, blasts};
    item.texture = material.floor.meta.id;
    item.border = material.wall.meta.id;
    item.texture_scale = material.floor.meta.size.x;
    item.tint = [255, 255, 255, 255];
    for (const entry of joined.slice(1)) entry.layer.items = entry.layer.items.filter(i => i !== entry.item);
    if (!joined.length) target.items.push(item);
    return item;
}

export function outsideCaveMasks(a: Point, b: Point, masks: Point[][], boundary = false): [Point, Point][] {
    const dx = b.x - a.x, dy = b.y - a.y, len = dx * dx + dy * dy;
    if (len < 1e-14) return [];
    const edges = masks.flatMap(r => r.map((p, n) => [p, r[(n + 1) % r.length]]));
    const ts = [0, 1];
    for (const [c, d] of edges) {
        const ex = d.x - c.x, ey = d.y - c.y, cx = c.x - a.x, cy = c.y - a.y, den = dx * ey - dy * ex;
        if (Math.abs(den) > 1e-12) {
            const t = (cx * ey - cy * ex) / den, u = (cx * dy - cy * dx) / den;
            if (t > 0 && t < 1 && u >= 0 && u <= 1) ts.push(t);
        } else if (Math.abs(cx * dy - cy * dx) < 1e-8) ts.push(...[c, d].map(p => Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / len))));
    }
    ts.sort((a, b) => a - b);
    const at = (t: number) => ({x: a.x + dx * t, y: a.y + dy * t});
    return ts.slice(1).flatMap((hi, n) => {
        const lo = ts[n];
        if (hi - lo < 1e-8) return [];
        const p = at((lo + hi) / 2);
        const onEdge = edges.some(([c, d]) => {
            const x = d.x - c.x, y = d.y - c.y, l = x * x + y * y,
                t = Math.max(0, Math.min(1, ((p.x - c.x) * x + (p.y - c.y) * y) / (l || 1)));
            return Math.hypot(p.x - c.x - x * t, p.y - c.y - y * t) < 1e-6;
        });
        let inside = false;
        for (const [c, d] of edges) if ((c.y > p.y) !== (d.y > p.y) && p.x < (d.x - c.x) * (p.y - c.y) / (d.y - c.y) + c.x) inside = !inside;
        return (onEdge ? boundary : inside) ? [] : [[at(lo), at(hi)] as [Point, Point]];
    });
}
