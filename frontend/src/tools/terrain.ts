import {type Item, type Layer, type Point, segmentDistance} from "../map/model";

function contains(rings: Point[][], p: Point) {
    let inside = false;
    for (const ring of rings) for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
        const a = ring[i], b = ring[j];
        if ((a.y > p.y) !== (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x) inside = !inside;
    }
    return inside;
}

export function terrainRegionsTouch(a: Point[][], b: Point[][]): boolean {
    const extent = (rings: Point[][]) => {
        let x = Infinity, y = Infinity, right = -Infinity, bottom = -Infinity;
        for (const ring of rings) for (const p of ring) {
            x = Math.min(x, p.x);
            y = Math.min(y, p.y);
            right = Math.max(right, p.x);
            bottom = Math.max(bottom, p.y);
        }
        return {x, y, right, bottom};
    };
    const ba = extent(a), bb = extent(b);
    if (ba.x > bb.right || bb.x > ba.right || ba.y > bb.bottom || bb.y > ba.bottom) return false;
    if (a.some(r => r.length && contains(b, r[0])) || b.some(r => r.length && contains(a, r[0]))) return true;
    const cross = (a: Point, b: Point, c: Point) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    for (const ra of a) for (let i = 0; i < ra.length; i++) {
        const p = ra[i], q = ra[(i + 1) % ra.length];
        for (const rb of b) for (let j = 0; j < rb.length; j++) {
            const r = rb[j], s = rb[(j + 1) % rb.length];
            if (Math.max(p.x, q.x) < Math.min(r.x, s.x) || Math.max(r.x, s.x) < Math.min(p.x, q.x)
                || Math.max(p.y, q.y) < Math.min(r.y, s.y) || Math.max(r.y, s.y) < Math.min(p.y, q.y)) continue;
            if (cross(p, q, r) * cross(p, q, s) < 0 && cross(r, s, p) * cross(r, s, q) < 0) return true;
            if (segmentDistance(p, r, s) < 1e-6 || segmentDistance(q, r, s) < 1e-6
                || segmentDistance(r, p, q) < 1e-6 || segmentDistance(s, p, q) < 1e-6) return true;
        }
    }
    return false;
}

// Only combine the final run of matching paint: crossing another material or
// a locked item would change the existing stacking order or protected geometry.
export function addTerrainRegion(layer: Layer, fresh: Item, radius: number): Item | undefined {
    if (layer.locked || !layer.visible || fresh.locked || fresh.water || fresh.cave || fresh.kind.type !== "terrain_region") return;
    const color = fresh.kind.color;
    const candidates: Item[] = [];
    for (let n = layer.items.length - 1; n >= 0; n--) {
        const item = layer.items[n];
        if (item.locked || item.water || item.cave || item.kind.type !== "terrain_region"
            || item.texture !== fresh.texture || item.border !== fresh.border || item.texture_scale !== fresh.texture_scale
            || !item.tint.every((v, i) => v === fresh.tint[i])
            || !item.kind.color.every((v, i) => v === color[i])) break;
        candidates.push(item);
    }
    const groups = [fresh.kind.rings];
    const joined = new Set<Item>();
    let changed = true;
    while (changed) {
        changed = false;
        for (const item of candidates) {
            if (joined.has(item) || item.kind.type !== "terrain_region") continue;
            const rings = item.kind.rings;
            if (!groups.some(group => terrainRegionsTouch(group, rings))) continue;
            groups.push(rings);
            joined.add(item);
            changed = true;
        }
    }
    if (!joined.size) {
        layer.items.push(fresh);
        return fresh;
    }
    // Rings within each region use even-odd fill; separate regions are united.
    const rings = terrainContours([], radius, {regions: groups, smoothing: 0});
    if (!rings.length) return;
    const item = layer.items.find(i => joined.has(i))!;
    item.kind = {type: "terrain_region", rings, color: [...color]};
    layer.items = layer.items.filter(i => !joined.has(i));
    layer.items.push(item);
    return item;
}

export function appendTerrainPoint(trail: Point[], p: Point, radius: number) {
    const last = trail.at(-1);
    if (!last) {
        trail.push(p);
        return;
    }
    const distance = Math.hypot(p.x - last.x, p.y - last.y);
    if (distance <= radius * 0.2) return;
    const count = Math.ceil(distance / (radius * 0.2));
    for (let i = 1; i <= count; i++) trail.push({
        x: last.x + (p.x - last.x) * i / count,
        y: last.y + (p.y - last.y) * i / count,
    });
}

// Persist the union boundary, including holes, rather than the brush's samples.
// Keep the sampling and smoothing consistent with geometry::terrain_contours
// used to migrate older projects in the engine.
export type WaterShape = "circle" | "square" | "polygon";

export function terrainContours(trail: Point[], radius: number, options: {
    shape?: WaterShape;
    existing?: Point[][];
    polygon?: Point[];
    regions?: Point[][][];
    smoothing?: number;
    subtract?: Point[][][];
} = {}): Point[][] {
    const groups = [options.existing ?? [], options.polygon ? [options.polygon] : [], ...(options.regions ?? [])];
    if ((!trail.length && !groups.some(g => g.length)) || !Number.isFinite(radius) || radius <= 0 ||
        trail.some(p => !Number.isFinite(p.x) || !Number.isFinite(p.y))) return [];
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const p of trail) {
        minX = Math.min(minX, p.x - radius);
        minY = Math.min(minY, p.y - radius);
        maxX = Math.max(maxX, p.x + radius);
        maxY = Math.max(maxY, p.y + radius);
    }
    for (const p of groups.flat(2)) {
        if (!Number.isFinite(p.x) || !Number.isFinite(p.y)) return [];
        minX = Math.min(minX, p.x);
        minY = Math.min(minY, p.y);
        maxX = Math.max(maxX, p.x);
        maxY = Math.max(maxY, p.y);
    }
    const step = Math.max(0.125, (maxX - minX) / 2046, (maxY - minY) / 2046);
    const nx = Math.ceil((maxX - minX) / step) + 2;
    const ny = Math.ceil((maxY - minY) / step) + 2;
    const x = minX - step, y = minY - step;
    const mask = new Uint8Array(nx * ny);
    for (const p of trail) {
        const x0 = Math.max(0, Math.floor((p.x - radius - x) / step));
        const x1 = Math.min(nx, Math.ceil((p.x + radius - x) / step));
        const y0 = Math.max(0, Math.floor((p.y - radius - y) / step));
        const y1 = Math.min(ny, Math.ceil((p.y + radius - y) / step));
        for (let row = y0; row < y1; row++)
            for (let col = x0; col < x1; col++)
                if (options.shape === "square"
                    ? Math.max(Math.abs(x + (col + 0.5) * step - p.x), Math.abs(y + (row + 0.5) * step - p.y)) <= radius
                    : Math.hypot(x + (col + 0.5) * step - p.x, y + (row + 0.5) * step - p.y) <= radius)
                    mask[row * nx + col] = 1;
    }
    // Union whole regions so extending a river never leaves a shore across it.
    // Each group uses even-odd fill to retain its islands.
    for (const [rings, value] of [...groups.map(r => [r, 1] as const), ...(options.subtract ?? []).map(r => [r, 0] as const)]) {
        for (let row = 0; row < ny; row++) {
            const scanY = y + (row + 0.5) * step;
            const intersections: number[] = [];
            for (const ring of rings) for (let i = 0; i < ring.length; i++) {
                const a = ring[i], b = ring[(i + 1) % ring.length];
                if ((a.y > scanY) !== (b.y > scanY))
                    intersections.push(a.x + (scanY - a.y) * (b.x - a.x) / (b.y - a.y));
            }
            intersections.sort((a, b) => a - b);
            for (let i = 0; i + 1 < intersections.length; i += 2) {
                const left = Math.min(nx, Math.max(0, Math.ceil((intersections[i] - x) / step - 0.5)));
                const right = Math.max(0, Math.min(nx, Math.ceil((intersections[i + 1] - x) / step - 0.5)));
                if (right > left) mask.fill(value, row * nx + left, row * nx + right);
            }
        }
    }
    const stride = nx + 1;
    const edges = new Map<number, number[]>();
    const edge = (a: number, b: number) => {
        const next = edges.get(a) ?? [];
        next.push(b);
        edges.set(a, next);
    };
    const filled = (col: number, row: number) => col >= 0 && row >= 0 && col < nx && row < ny && mask[row * nx + col];
    for (let row = 0; row < ny; row++) {
        for (let col = 0; col < nx; col++) {
            if (!filled(col, row)) continue;
            const a = row * stride + col;
            if (!filled(col, row - 1)) edge(a, a + 1);
            if (!filled(col + 1, row)) edge(a + 1, a + stride + 1);
            if (!filled(col, row + 1)) edge(a + stride + 1, a + stride);
            if (!filled(col - 1, row)) edge(a + stride, a);
        }
    }
    const point = (v: number): Point => ({x: x + v % stride * step, y: y + Math.floor(v / stride) * step});
    const rings: Point[][] = [];
    while (edges.size) {
        const start = edges.keys().next().value!;
        let current = start;
        let previous = {x: current % stride - 1, y: Math.floor(current / stride)};
        let ring: Point[] = [];
        do {
            ring.push(point(current));
            const next = edges.get(current);
            if (!next?.length) break;
            const cx = current % stride, cy = Math.floor(current / stride);
            const dx = cx - previous.x, dy = cy - previous.y;
            const score = (v: number) => {
                const ox = v % stride - cx, oy = Math.floor(v / stride) - cy;
                const cross = dx * oy - dy * ox;
                return cross > 0 ? 3 : dx * ox + dy * oy > 0 ? 2 : cross < 0 ? 1 : 0;
            };
            let best = 0;
            for (let i = 1; i < next.length; i++) if (score(next[i]) > score(next[best])) best = i;
            const vertex = next.splice(best, 1)[0];
            if (!next.length) edges.delete(current);
            previous = {x: cx, y: cy};
            current = vertex;
        } while (current !== start && ring.length <= mask.length * 4);
        ring = ring.filter((b, i, points) => {
            const a = points[(i + points.length - 1) % points.length], c = points[(i + 1) % points.length];
            return Math.abs((b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x)) > 0.00001;
        });
        for (let pass = 0; pass < (options.smoothing !== undefined || options.shape && options.shape !== "circle" ? 0 : 2); pass++) {
            ring = ring.flatMap((a, i, points) => {
                const b = points[(i + 1) % points.length];
                return [
                    {x: a.x * 0.75 + b.x * 0.25, y: a.y * 0.75 + b.y * 0.25},
                    {x: a.x * 0.25 + b.x * 0.75, y: a.y * 0.25 + b.y * 0.75},
                ];
            });
        }
        if (ring.length >= 3) rings.push(ring);
    }
    return options.smoothing ? smoothWaterContours(rings, options.smoothing, radius) : rings;
}

// Smooth a uniformly sampled outline, so long straight edges and short raster
// steps receive the same treatment. The brush radius limits corner rounding.
export function smoothWaterContours(rings: Point[][], amount: number, radius: number): Point[][] {
    const reach = Math.min(radius * 0.3, Math.max(0, Math.min(1, amount)) * 0.65);
    if (!reach) return rings;
    return rings.map(ring => {
        const lengths = ring.map((p, i) => Math.hypot(p.x - ring[(i + 1) % ring.length].x, p.y - ring[(i + 1) % ring.length].y));
        const perimeter = lengths.reduce((a, b) => a + b, 0);
        if (!perimeter) return ring;
        const count = Math.min(8192, Math.max(8, Math.ceil(perimeter / Math.min(0.08, radius * 0.15))));
        const spacing = perimeter / count;
        const samples: Point[] = [];
        let edge = 0, distance = 0;
        for (let i = 0; i < count; i++) {
            const d = i * spacing;
            while (edge < ring.length - 1 && distance + lengths[edge] < d) distance += lengths[edge++];
            const a = ring[edge], b = ring[(edge + 1) % ring.length], t = (d - distance) / (lengths[edge] || 1);
            samples.push({x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t});
        }
        const window = Math.min(Math.floor(count / 4), Math.max(1, Math.ceil(reach / spacing)));
        return samples.map((_, i) => {
            let x = 0, y = 0, total = 0;
            for (let j = -window; j <= window; j++) {
                const weight = Math.exp(-2 * (j / window) ** 2);
                const p = samples[(i + j + count) % count];
                x += p.x * weight;
                y += p.y * weight;
                total += weight;
            }
            return {x: x / total, y: y / total};
        });
    });
}
