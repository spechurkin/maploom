import type {Item, Level, Point} from "../map/model";
import {corners, prefabParts} from "../map/model";
import {caveFootprint, outsideCaveMasks} from "./caves";
import {terrainContours} from "./terrain";

export type WallSegment = { a: Point; b: Point; width: number };
export type Silhouettes = ReadonlyMap<string, readonly Point[]>;
const dot = (a: Point, b: Point) => a.x * b.x + a.y * b.y;

/** Visible wall faces, including room holes, with door/window openings removed. */
export function snapWalls(level: Level, excluded: ReadonlySet<string> = new Set()): WallSegment[] {
    const items = level.layers.filter(l => l.visible).flatMap(l => l.items).filter(i => !excluded.has(i.id));
    const portals = items.filter(i => i.kind.type === "portal");
    const openings = portals.flatMap(i => i.kind.type === "portal" && !i.wall_attachment ? [i.kind.bounds] : []);
    const walls: WallSegment[] = [];
    const caveRegions = items.filter(i => i.cave).map(caveFootprint);
    const caveMask = caveRegions.length ? terrainContours([], 1, {regions: caveRegions, smoothing: 0}) : [];
    for (const item of items) {
        const k = item.kind;
        const rings = k.type === "wall" ? [k.points] : k.type === "room" && k.wall
            ? k.rings.filter((_, n) => !(item.wall_exclusions ?? []).includes(n)) : [];
        const width = k.type === "wall" || k.type === "room" ? k.width : 0;
        const add = (a: Point, b: Point) => outsideCaveMasks(a, b, k.type === "room" ? caveMask : [])
            .forEach(([a, b]) => walls.push({a, b, width}));
        let first = 0;
        for (const ring of rings) {
            const count = ring.length - (k.type === "wall" ? 1 : 0);
            const distances = [0];
            for (let n = 0; n < count; n++) distances.push(distances[n] + Math.hypot(
                ring[(n + 1) % ring.length].x - ring[n].x, ring[(n + 1) % ring.length].y - ring[n].y));
            const total = distances[count];
            const attached = portals.flatMap(i => {
                const a = i.wall_attachment;
                if (i.kind.type !== "portal" || a?.wall !== item.id || a.segment < first || a.segment >= first + count) return [];
                // The engine already resolved the chord endpoints on this
                // contour. Locate them here instead of approximating a curve
                // by matching the straight door against nearby wall edges.
                const locate = (p: Point) => {
                    let best = Infinity, result = 0;
                    for (let n = 0; n < count; n++) {
                        const q = ring[n], r = ring[(n + 1) % ring.length], len = distances[n + 1] - distances[n];
                        if (len < 1e-8) continue;
                        const t = Math.max(0, Math.min(1, ((p.x - q.x) * (r.x - q.x) + (p.y - q.y) * (r.y - q.y)) / (len * len)));
                        const distance = Math.hypot(p.x - q.x - t * (r.x - q.x), p.y - q.y - t * (r.y - q.y));
                        if (distance < best) {
                            best = distance;
                            result = distances[n] + t * len;
                        }
                    }
                    return best < 1e-4 ? result : null;
                };
                const [lo, hi] = i.kind.bounds.map(locate);
                if (lo === null || hi === null) return [];
                return lo <= hi ? [[lo, hi]] : [[lo, total], [0, hi]];
            });
            for (let n = 0; n < count; n++) {
                const a = ring[n], b = ring[(n + 1) % ring.length];
                const v = {x: b.x - a.x, y: b.y - a.y}, len2 = dot(v, v), len = Math.sqrt(len2);
                if (len < 1e-8) continue;
                const cuts = [...attached.flatMap(([start, end]) => {
                    const lo = Math.max(0, (start - distances[n]) / len), hi = Math.min(1, (end - distances[n]) / len);
                    return hi > lo ? [[lo, hi]] : [];
                }), ...openings.flatMap(pair => {
                    if (!pair.every(p => Math.abs((p.x - a.x) * v.y - (p.y - a.y) * v.x) / len < 0.025)) return [];
                    const ts = pair.map(p => ((p.x - a.x) * v.x + (p.y - a.y) * v.y) / len2);
                    const lo = Math.max(0, Math.min(...ts)), hi = Math.min(1, Math.max(...ts));
                    return hi > lo ? [[lo, hi]] : [];
                })].sort((x, y) => x[0] - y[0]);
                const at = (t: number) => ({x: a.x + t * v.x, y: a.y + t * v.y});
                let cursor = 0;
                for (const [lo, hi] of cuts) {
                    if (lo > cursor) add(at(cursor), at(lo));
                    cursor = Math.max(cursor, hi);
                }
                if (cursor < 1) add(at(cursor), b);
            }
            first += count;
        }
    }
    return walls;
}

/** Transform the alpha silhouette, independently of the selection rectangle. */
export function objectSilhouette(item: Item, hull: readonly Point[], delta: Point): Point[] {
    const k = item.kind;
    if (k.type !== "object") return [];
    const c = Math.cos(k.rotation), s = Math.sin(k.rotation);
    return hull.map(p => {
        const x = ((k.flip_x ? 1 - p.x : p.x) - k.pivot.x) * k.size.x;
        const y = ((k.flip_y ? 1 - p.y : p.y) - k.pivot.y) * k.size.y;
        return {x: k.position.x + delta.x + x * c - y * s, y: k.position.y + delta.y + x * s + y * c};
    });
}

/** Wall snapping wins near a wall; elsewhere the caller retains its grid position.
 * The gap is five map pixels, independent of viewport zoom and object scale.
 * A group receives one translation so relative positions remain intact.
 */
export function snapObjectsToWalls(items: readonly Item[], delta: Point, walls: readonly WallSegment[],
                                   hulls: Silhouettes, pixelsPerGrid: number, tolerance: number): Point | null {
    if (!items.length || items.some(i => i.kind.type !== "object" || (!i.prefab && !hulls.get(i.kind.asset)?.length))) return null;
    const points = items.flatMap(i => i.prefab ? prefabParts(i).flatMap(part => {
        const hull = part.kind.type === "object" ? hulls.get(part.kind.asset) : undefined;
        return hull?.length ? objectSilhouette(part, hull, delta)
            : corners(part).map(p => ({x: p.x + delta.x, y: p.y + delta.y}));
    }) : objectSilhouette(i, hulls.get(i.kind.type === "object" ? i.kind.asset : "")!, delta));
    if (!points.length) return null;
    const gap = 5 / Math.max(1, pixelsPerGrid);
    const contacts = walls.flatMap(wall => {
        const length = Math.hypot(wall.b.x - wall.a.x, wall.b.y - wall.a.y);
        if (length < 1e-8) return [];
        const tangent = {x: (wall.b.x - wall.a.x) / length, y: (wall.b.y - wall.a.y) / length};
        const normal = {x: -tangent.y, y: tangent.x};
        const ns = points.map(p => dot({x: p.x - wall.a.x, y: p.y - wall.a.y}, normal));
        const min = Math.min(...ns), max = Math.max(...ns);
        const positive = min + max >= 0;
        const distance = positive ? wall.width / 2 + gap - min : -wall.width / 2 - gap - max;
        if (Math.abs(distance) > tolerance) return [];
        const overlaps = (shift: Point) => {
            // Only the silhouette's supporting edge/vertices can touch this face.
            const contact = points.filter((_, n) => Math.abs(ns[n] - (positive ? min : max)) < 1e-7);
            const ts = contact.map(p => dot({x: p.x + shift.x - wall.a.x, y: p.y + shift.y - wall.a.y}, tangent));
            return Math.max(...ts) >= 0 && Math.min(...ts) <= length;
        };
        const shift = {x: normal.x * distance, y: normal.y * distance};
        return overlaps(shift) ? [{normal, distance, shift, overlaps}] : [];
    });
    let best: Point | null = null, score = Infinity;
    for (const c of contacts) {
        const cost = Math.hypot(c.shift.x, c.shift.y);
        if (cost < score) {
            best = c.shift;
            score = cost;
        }
    }
    // Snap into corners without one wall correction undoing the other.
    let cornerScore = Infinity;
    for (let i = 0; i < contacts.length; i++) for (let j = i + 1; j < contacts.length; j++) {
        const a = contacts[i], b = contacts[j], det = a.normal.x * b.normal.y - a.normal.y * b.normal.x;
        if (Math.abs(det) < 0.1) continue;
        const shift = {
            x: (a.distance * b.normal.y - a.normal.y * b.distance) / det,
            y: (a.normal.x * b.distance - a.distance * b.normal.x) / det
        };
        const cost = Math.hypot(shift.x, shift.y);
        if (cost <= tolerance && cost < cornerScore && a.overlaps(shift) && b.overlaps(shift)) {
            best = shift;
            cornerScore = cost;
        }
    }
    return best ? {x: delta.x + best.x, y: delta.y + best.y} : null;
}
