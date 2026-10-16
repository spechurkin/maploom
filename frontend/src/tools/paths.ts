import type {AssetMeta, Item, Point} from "../map/model";

/** Keep the user's repeat multiplier when replacing a ribbon's texture. */
export function setPathTexture(item: Item, texture: AssetMeta, previous?: AssetMeta) {
    if (item.kind.type !== "path") return;
    const width = item.kind.width;
    const repeat = (asset: AssetMeta) => asset.size.x / Math.max(0.01, asset.size.y) * width;
    const previousRepeat = previous ? repeat(previous) : 0;
    const multiplier = previousRepeat > 0 ? item.texture_scale / previousRepeat : 1;
    item.texture = texture.id;
    item.texture_scale = repeat(texture) * multiplier;
}

export type PathTransition = "none" | "fade" | "shrink";
export type PathStyle = {
    smoothness: number;
    transition_in: PathTransition;
    transition_out: PathTransition;
    blocks_light: boolean
};
export const defaultPathStyle = (): PathStyle => ({
    smoothness: 0,
    transition_in: "none",
    transition_out: "none",
    blocks_light: false
});
export type PathVertex = { position: Point; uv: Point; alpha: number };
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);

/** Interpolating cardinal spline; zero keeps the original polyline, including legacy maps. */
export function pathPoints(points: Point[], smoothness = 0): Point[] {
    const ps = points.filter((p, n) => n === 0 || distance(p, points[n - 1]) > 1e-5);
    if (ps.length < 2) return ps;
    const out = [{...ps[0]}], s = Math.max(0, Math.min(1, smoothness));
    for (let n = 0; n < ps.length - 1; n++) {
        const a = ps[n], b = ps[n + 1], prev = ps[Math.max(0, n - 1)], next = ps[Math.min(ps.length - 1, n + 2)];
        const steps = Math.max(8, Math.min(64, Math.ceil(distance(a, b) / 0.15)));
        for (let k = 1; k <= steps; k++) {
            const t = k / steps, t2 = t * t, t3 = t2 * t;
            const coordinate = (axis: "x" | "y") => {
                const curve = (2 * t3 - 3 * t2 + 1) * a[axis] + (t3 - 2 * t2 + t) * (b[axis] - prev[axis]) / 2
                    + (-2 * t3 + 3 * t2) * b[axis] + (t3 - t2) * (next[axis] - a[axis]) / 2;
                return (a[axis] + (b[axis] - a[axis]) * t) * (1 - s) + curve * s;
            };
            out.push({x: coordinate("x"), y: coordinate("y")});
        }
    }
    return out;
}

/** One joined ribbon with cumulative UVs and bounded miters, also used for selection and previews. */
export function pathRibbon(points: Point[], width: number, scale: number, style: PathStyle, profile?: Item["path_profile"]): PathVertex[] {
    if (profile?.widths.length !== points.length || profile?.alpha.length !== points.length) profile = undefined;
    let ps = profile ? points : pathPoints(points, style.smoothness);
    if (ps.length < 2) return [];
    const total = ps.slice(1).reduce((sum, p, n) => sum + distance(p, ps[n]), 0),
        transition = Math.min(width, total / 2);
    // Exact transition samples keep short end effects short even on very long segments.
    const cuts = [...new Set([0.25, 0.5, 0.75, 1].flatMap(t => [
        ...(style.transition_in !== "none" ? [transition * t] : []),
        ...(style.transition_out !== "none" ? [total - transition * t] : [])]))].sort((a, b) => a - b);
    if (!profile && cuts.length) {
        const refined = [ps[0]];
        let travelled = 0;
        for (let n = 1; n < ps.length; n++) {
            const a = ps[n - 1], b = ps[n], len = distance(a, b);
            for (const cut of cuts) if (cut > travelled + 1e-6 && cut < travelled + len - 1e-6) {
                const t = (cut - travelled) / len;
                refined.push({x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t});
            }
            refined.push(b);
            travelled += len;
        }
        ps = refined;
    }
    const lengths = ps.slice(1).map((p, n) => distance(p, ps[n]));
    const normal = (a: Point, b: Point) => {
        const d = Math.max(1e-7, distance(a, b));
        return {x: -(b.y - a.y) / d, y: (b.x - a.x) / d};
    };
    let travelled = 0;
    return ps.flatMap((p, n) => {
        if (n) travelled += lengths[n - 1];
        const incoming = normal(ps[Math.max(0, n - 1)], n ? p : ps[1]);
        const outgoing = n === ps.length - 1 ? incoming : normal(p, ps[n + 1]);
        const sx = incoming.x + outgoing.x, sy = incoming.y + outgoing.y, len = Math.hypot(sx, sy);
        const avg = len > 1e-5 ? {x: sx / len, y: sy / len} : outgoing;
        const divisor = Math.max(0.5, avg.x * outgoing.x + avg.y * outgoing.y);
        const start = Math.min(1, travelled / Math.max(1e-7, transition)),
            end = Math.min(1, (total - travelled) / Math.max(1e-7, transition));
        const factor = profile ? profile.widths[n] : (style.transition_in === "shrink" ? start : 1) * (style.transition_out === "shrink" ? end : 1);
        const alpha = profile ? profile.alpha[n] : (style.transition_in === "fade" ? start : 1) * (style.transition_out === "fade" ? end : 1);
        return [1, -1].map((sign, row) => ({
            position: {
                x: p.x + avg.x / divisor * width / 2 * sign * factor,
                y: p.y + avg.y / divisor * width / 2 * sign * factor
            },
            uv: {x: travelled / Math.max(0.01, scale), y: row}, alpha
        }));
    });
}

export function itemPathRibbon(item: Item) {
    return item.kind.type === "path" ? pathRibbon(item.kind.points, item.kind.width, item.texture_scale, {...defaultPathStyle(), ...item.path_style}, item.path_profile) : [];
}

/** Reduce pointer noise without discarding corners or the end of a stroke. */
export function simplifyPath(points: Point[], tolerance: number): Point[] {
    if (points.length < 3) return points.map(p => ({...p}));
    const keep = new Set([0, points.length - 1]), stack = [[0, points.length - 1]];
    while (stack.length) {
        const [start, end] = stack.pop()!, a = points[start], b = points[end];
        const dx = b.x - a.x, dy = b.y - a.y, d2 = dx * dx + dy * dy;
        let far = tolerance, index = -1;
        for (let n = start + 1; n < end; n++) {
            const p = points[n], t = d2 ? Math.max(0, Math.min(1, ((p.x - a.x) * dx + (p.y - a.y) * dy) / d2)) : 0;
            const d = Math.hypot(p.x - a.x - t * dx, p.y - a.y - t * dy);
            if (d > far) {
                far = d;
                index = n;
            }
        }
        if (index >= 0) {
            keep.add(index);
            stack.push([start, index], [index, end]);
        }
    }
    return [...keep].sort((a, b) => a - b).map(n => ({...points[n]}));
}
