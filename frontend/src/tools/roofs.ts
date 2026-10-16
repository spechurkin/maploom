import type {Asset, Item, Point} from "../map/model";

export type RoofType = "gable" | "hip" | "dormer";
export type RoofKind = {
    type: "roof"; points: Point[]; width: number; roof_type: RoofType;
    ridge: string | null; edge: string | null; hip: string | null;
    shade: boolean; sun_direction: number; contrast: number;
};
const add = (a: Point, b: Point) => ({x: a.x + b.x, y: a.y + b.y});
const sub = (a: Point, b: Point) => ({x: a.x - b.x, y: a.y - b.y});
const mul = (p: Point, s: number) => ({x: p.x * s, y: p.y * s});
const dot = (a: Point, b: Point) => a.x * b.x + a.y * b.y;
const distance = (a: Point, b: Point) => Math.hypot(a.x - b.x, a.y - b.y);

export function roofGeometry(k: Pick<RoofKind, "points" | "width" | "roof_type">) {
    const {points, width, roof_type} = k, n = points.length - 1;
    const outline: Point[] = [], facets: { points: Point[]; down: Point }[] = [], ridge = points.map(p => ({...p})),
        hips: Point[][] = [];
    if (n < 1 || width <= 0) return {outline, facets, ridge, hips};
    const half = width / 2;
    const ds = points.slice(1).map((p, i) => mul(sub(p, points[i]), 1 / Math.max(1e-7, distance(p, points[i]))));
    const ns = ds.map(d => ({x: -d.y, y: d.x}));
    const left: Point[] = [], right: Point[] = [];
    points.forEach((p, i) => {
        const offset = i === 0 ? mul(ns[0], half) : i === n ? mul(ns[n - 1], half)
            : mul(add(ns[i - 1], ns[i]), half / Math.max(1e-6, 1 + dot(ns[i - 1], ns[i])));
        left.push(add(p, offset));
        right.push(sub(p, offset));
    });
    if (roof_type === "hip") {
        ridge[0] = add(points[0], mul(ds[0], Math.min(half, distance(points[0], points[1]) / 2)));
        ridge[n] = sub(points[n], mul(ds[n - 1], Math.min(half, distance(points[n], points[n - 1]) / 2)));
        for (const [i, down] of [[0, mul(ds[0], -1)], [n, ds[n - 1]]] as const) {
            facets.push({points: [left[i], right[i], ridge[i]], down});
            hips.push([left[i], ridge[i]], [right[i], ridge[i]]);
        }
    } else if (roof_type === "dormer") {
        const p = sub(points[n], mul(ds[n - 1], Math.min(half, distance(points[n], points[n - 1]) * 0.9)));
        left[n] = add(p, mul(ns[n - 1], half));
        right[n] = sub(p, mul(ns[n - 1], half));
    }
    for (let i = 0; i < n; i++) for (const [edge, sign] of [[left, 1], [right, -1]] as const)
        facets.push({points: [edge[i], edge[i + 1], ridge[i + 1], ridge[i]], down: mul(ns[i], sign)});
    outline.push(...left, ...(roof_type === "dormer" ? [points[n]] : []), ...right.reverse());
    return {outline, facets, ridge, hips};
}

export function roofQuickbox(a: Point, b: Point) {
    const x = Math.min(a.x, b.x), y = Math.min(a.y, b.y), w = Math.abs(a.x - b.x), h = Math.abs(a.y - b.y);
    return w >= h ? {points: [{x, y: y + h / 2}, {x: x + w, y: y + h / 2}], width: h}
        : {points: [{x: x + w / 2, y}, {x: x + w / 2, y: y + h}], width: w};
}

export function roofStyleAsset(asset: Asset) {
    return asset.category === "roofs" && !/^(edge|hip|ridge)\.[^/]+$/i.test(asset.meta.id.split(/[\\/]/).at(-1) ?? "");
}

export function roofStyleName(asset: Asset) {
    const parts = asset.meta.id.split(/[\\/]/);
    return /^tiles\./i.test(parts.at(-1) ?? "") ? parts.at(-2)!.replaceAll("_", " ") : asset.meta.name.replaceAll("_", " ");
}

export function roofStyle(asset: Asset, assets: Asset[]) {
    const sibling = (part: string) => assets.find(a => a.meta.id === asset.meta.id.replace(/[^/\\]+$/, part + "." + asset.meta.id.split(".").at(-1)))?.meta.id ?? null;
    return {ridge: sibling("ridge"), edge: sibling("edge"), hip: sibling("hip")};
}

export function newRoof(points: Point[], width: number, roof_type: RoofType, asset: Asset, assets: Asset[]): RoofKind {
    return {
        type: "roof",
        points,
        width,
        roof_type, ...roofStyle(asset, assets),
        shade: true,
        sun_direction: 135,
        contrast: 0.3
    };
}

export const isRoof = (item: Item): item is Item & { kind: RoofKind } => item.kind.type === "roof";
