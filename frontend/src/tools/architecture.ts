import {bounds, type Item, type Point, segmentDistance} from "../map/model";
import {roofGeometry} from "./roofs";

export type DrawShape = "rectangle" | "circle" | "polygon";
export type Vertex = { ring: number; point: number };
export type GeometryMode = "points" | "resize" | "hole";
export const supportsGeometry = (item: Item) => ["wall", "floor", "room", "roof", "path"].includes(item.kind.type);
const lineGeometry = (item: Item) => item.kind.type === "wall" || item.kind.type === "roof" || item.kind.type === "path";

export function geometryRings(item: Item): Point[][] {
    const k = item.kind;
    if (k.type === "room") return k.rings;
    if (k.type === "floor") return [k.points, ...(item.floor_holes ?? [])];
    if (k.type === "wall" || k.type === "roof" || k.type === "path") return [k.points];
    return [];
}

export const signedArea = (ring: Point[]) => ring.reduce((sum, a, n) => {
    const b = ring[(n + 1) % ring.length];
    return sum + a.x * b.y - a.y * b.x;
}, 0) / 2;
export const samePoint = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y) < 1e-7;

export function oriented(ring: Point[], positive: boolean) {
    return (signedArea(ring) > 0) === positive ? ring : [...ring].reverse();
}

export function rectangle(a: Point, b: Point): Point[] {
    return [{x: Math.min(a.x, b.x), y: Math.min(a.y, b.y)}, {x: Math.max(a.x, b.x), y: Math.min(a.y, b.y)},
        {x: Math.max(a.x, b.x), y: Math.max(a.y, b.y)}, {x: Math.min(a.x, b.x), y: Math.max(a.y, b.y)}];
}

export function closedRing(item: Item, ring: Point[]) {
    return !lineGeometry(item) || samePoint(ring[0], ring.at(-1)!);
}

export function vertexHandles(item: Item) {
    return geometryRings(item).flatMap((ring, r) => ring.map((p, n) => ({position: p, ring: r, point: n}))
        .filter((_, n) => !(item.kind.type === "wall" && n === ring.length - 1 && closedRing(item, ring))));
}

export function midpointHandles(item: Item) {
    return geometryRings(item).flatMap((ring, r) => ring.flatMap((a, n) => {
        if (lineGeometry(item) && n === ring.length - 1) return [];
        const b = ring[(n + 1) % ring.length];
        return [{position: {x: (a.x + b.x) / 2, y: (a.y + b.y) / 2}, ring: r, point: n + 1}];
    }));
}

export type GeometryTarget = Vertex & { type: "vertex" | "edge"; position: Point };

/** Pick in screen pixels, projecting edge clicks onto the actual contour. */
export function geometryTarget(item: Item, position: Point, scale: number): GeometryTarget | null {
    const distance = (p: Point) => Math.hypot(position.x - p.x, position.y - p.y);
    const vertex = vertexHandles(item).sort((a, b) => distance(a.position) - distance(b.position))[0];
    if (vertex && distance(vertex.position) <= 8 / scale) return {...vertex, type: "vertex"};
    let target: GeometryTarget | null = null, nearest = Infinity;
    geometryRings(item).forEach((ring, r) => {
        const k = item.kind;
        const tolerance = 2 / scale;
        const count = k.type === "wall" || k.type === "roof" || k.type === "path" ? ring.length - 1 : ring.length;
        for (let n = 0; n < count; n++) {
            const a = ring[n], b = ring[(n + 1) % ring.length];
            const dx = b.x - a.x, dy = b.y - a.y, length = dx * dx + dy * dy;
            if (length < 1e-14) continue;
            const t = Math.max(0, Math.min(1, ((position.x - a.x) * dx + (position.y - a.y) * dy) / length));
            // Endpoints already have handles; never insert a duplicate vertex.
            if (t <= 1e-7 || t >= 1 - 1e-7) continue;
            const projected = {x: a.x + t * dx, y: a.y + t * dy}, d = distance(projected);
            if (d <= tolerance && d < nearest) {
                nearest = d;
                target = {type: "edge", position: projected, ring: r, point: n + 1};
            }
        }
    });
    return target;
}

export function moveVertex(item: Item, vertex: Vertex, position: Point) {
    const ring = geometryRings(item)[vertex.ring];
    if (!ring?.[vertex.point]) return;
    const closed = item.kind.type === "wall" && closedRing(item, ring);
    ring[vertex.point] = {...position};
    if (closed && vertex.point === 0) ring[ring.length - 1] = {...position};
}

export function insertVertex(item: Item, vertex: Vertex, position: Point) {
    geometryRings(item)[vertex.ring]?.splice(vertex.point, 0, {...position});
}

export function removeVertex(item: Item, vertex: Vertex): boolean {
    const ring = geometryRings(item)[vertex.ring];
    if (!ring) return false;
    const closed = item.kind.type === "wall" && closedRing(item, ring);
    if (ring.length <= (lineGeometry(item) ? closed ? 4 : 2 : 3)) return false;
    ring.splice(vertex.point, 1);
    if (closed) ring[ring.length - 1] = {...ring[0]};
    return true;
}

export function resizeGeometry(item: Item, width: number, height: number) {
    const b = bounds(item), w = b.right - b.x, h = b.bottom - b.y;
    for (const p of geometryRings(item).flat()) {
        p.x = b.x + (w > 1e-7 ? (p.x - b.x) * width / w : 0);
        p.y = b.y + (h > 1e-7 ? (p.y - b.y) * height / h : 0);
    }
}

export function addHole(item: Item, ring: Point[], walls: boolean) {
    const hole = oriented(ring, false);
    if (item.kind.type === "floor") (item.floor_holes ??= []).push(hole);
    if (item.kind.type === "room") {
        const index = item.kind.rings.length;
        item.kind.rings.push(hole);
        if (!walls) (item.wall_exclusions ??= []).push(index);
    }
}

export function removeHole(item: Item, index: number) {
    if (item.kind.type === "floor") item.floor_holes?.splice(index - 1, 1);
    if (item.kind.type === "room") {
        if (signedArea(item.kind.rings[index]) >= 0) return;
        item.kind.rings.splice(index, 1);
        item.wall_exclusions = (item.wall_exclusions ?? []).filter(n => n !== index).map(n => n > index ? n - 1 : n);
    }
}

export function setContourWalls(item: Item, index: number, enabled: boolean) {
    item.wall_exclusions = (item.wall_exclusions ?? []).filter(n => n !== index);
    if (!enabled) item.wall_exclusions.push(index);
}

function contains(ring: Point[], p: Point) {
    let inside = false;
    for (let n = 0, j = ring.length - 1; n < ring.length; j = n++) {
        const a = ring[n], b = ring[j];
        if ((a.y > p.y) !== (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) inside = !inside;
    }
    return inside;
}

function intersects(a: Point, b: Point, c: Point, d: Point) {
    if ([segmentDistance(a, c, d), segmentDistance(b, c, d), segmentDistance(c, a, b), segmentDistance(d, a, b)].some(n => n < 1e-7)) return true;
    const cross = (p: Point, q: Point, r: Point) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    return cross(a, b, c) * cross(a, b, d) < 0 && cross(c, d, a) * cross(c, d, b) < 0;
}

export function validGeometry(item: Item): boolean {
    const rings = geometryRings(item);
    if (!rings.length || rings.flat().some(p => !Number.isFinite(p.x) || !Number.isFinite(p.y))) return false;
    if (item.kind.type === "roof") {
        const k = item.kind;
        if (!Number.isFinite(k.width) || k.width < 0.05 || k.width > 512 || k.points.length < 2 ||
            k.points.slice(1).some((p, n) => Math.hypot(p.x - k.points[n].x, p.y - k.points[n].y) < 0.01)) return false;
        return validGeometry({
            ...item,
            kind: {type: "floor", points: roofGeometry(k).outline, color: [255, 255, 255, 255]},
            floor_holes: []
        });
    }
    if (item.kind.type === "path" && (!Number.isFinite(item.kind.width) || item.kind.width <= 0 || item.kind.width > 512 ||
        !Number.isFinite(item.path_style?.smoothness ?? 0) || (item.path_style?.smoothness ?? 0) < 0 || (item.path_style?.smoothness ?? 0) > 1)) return false;
    if (item.kind.type === "wall" || item.kind.type === "path") return rings[0].length >= 2 && rings[0].slice(1).every((p, n) => !samePoint(p, rings[0][n]));
    for (const ring of rings) {
        if (ring.length < 3 || Math.abs(signedArea(ring)) < 1e-7) return false;
        for (let n = 0; n < ring.length; n++) {
            if (samePoint(ring[n], ring[(n + 1) % ring.length])) return false;
            for (let j = n + 1; j < ring.length; j++) {
                if (j === n + 1 || (n === 0 && j === ring.length - 1)) continue;
                if (intersects(ring[n], ring[(n + 1) % ring.length], ring[j], ring[(j + 1) % ring.length])) return false;
            }
        }
    }
    const holes = rings.filter((r, n) => item.kind.type === "floor" ? n > 0 : signedArea(r) < 0);
    const outers = rings.filter(r => !holes.includes(r));
    if (!outers.length) return false;
    for (let n = 0; n < rings.length; n++) for (let j = n + 1; j < rings.length; j++) {
        const a = rings[n], b = rings[j];
        if (a.some((p, i) => b.some((q, k) => intersects(p, a[(i + 1) % a.length], q, b[(k + 1) % b.length])))) return false;
        if (holes.includes(a) === holes.includes(b) && (contains(a, b[0]) || contains(b, a[0]))) return false;
    }
    return holes.every(h => outers.filter(o => contains(o, h[0])).length === 1);
}

export function circleContour(center: Point, edge: Point, segmentation: number): Point[] {
    const radius = Math.hypot(edge.x - center.x, edge.y - center.y);
    const sides = segmentation >= 64 ? 512 : Math.max(6, Math.round(segmentation));
    return Array.from({length: sides}, (_, n) => {
        const angle = n * 2 * Math.PI / sides;
        return {x: center.x + Math.cos(angle) * radius, y: center.y + Math.sin(angle) * radius};
    });
}

export function resizeFromCorner(item: Item, corner: number, p: Point) {
    const b = bounds(item), anchors = rectangle({x: b.x, y: b.y}, {x: b.right, y: b.bottom});
    const anchor = anchors[(corner + 2) % 4];
    const left = corner === 0 || corner === 3, top = corner < 2;
    const x = left ? Math.min(p.x, anchor.x - 0.05) : Math.max(p.x, anchor.x + 0.05);
    const y = top ? Math.min(p.y, anchor.y - 0.05) : Math.max(p.y, anchor.y + 0.05);
    const w = b.right - b.x, h = b.bottom - b.y;
    for (const q of geometryRings(item).flat()) {
        if (w > 1e-7) q.x = anchor.x + (q.x - anchor.x) * Math.abs(x - anchor.x) / w;
        if (h > 1e-7) q.y = anchor.y + (q.y - anchor.y) * Math.abs(y - anchor.y) / h;
    }
}
