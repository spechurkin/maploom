import {type AssetMeta, type Color, type Item, newItem, type Point} from "../map/model";

export type Interval = [number, number];
export type ScatterSettings = {
    rotation: Interval;
    scale: Interval;
    spread: number;
    randomColors: boolean;
    colors: Color[];
    color: Color;
};

export function chooseScatterAssets(ids: string[], id: string, additive: boolean): string[] {
    return additive ? ids.includes(id) ? ids.filter(value => value !== id) : [...ids, id] : [id];
}

export function scatterItem(assets: AssetMeta[], position: Point, settings: ScatterSettings,
                            random = Math.random): Item | null {
    if (!assets.length) return null;
    const asset = assets[Math.min(assets.length - 1, Math.floor(random() * assets.length))];
    const between = ([min, max]: Interval) => min + random() * (max - min);
    const scale = between(settings.scale);
    const item = newItem(asset.name, {
        type: "object", asset: asset.id, position: {...position},
        size: {x: asset.size.x * scale, y: asset.size.y * scale},
        rotation: between(settings.rotation) * Math.PI / 180,
        pivot: {...asset.pivot}, flip_x: false, flip_y: false,
    });
    item.tint = asset.color_mode === "none" ? [255, 255, 255, 255]
        : [...(settings.randomColors && settings.colors.length
            ? settings.colors[Math.min(settings.colors.length - 1, Math.floor(random() * settings.colors.length))]
            : settings.color)];
    return item;
}

export type ScatterStroke = { last: Point; remaining: number };

// Carry the unused distance across pointer events, so density does not depend
// on pointer speed, event frequency, zoom, or grid snapping.
export function extendScatterStroke(stroke: ScatterStroke, to: Point, spread: number): Point[] {
    if (!Number.isFinite(spread) || spread <= 0) return [];
    const from = stroke.last;
    const length = Math.hypot(to.x - from.x, to.y - from.y);
    if (!length) return [];
    const points: Point[] = [];
    let distance = stroke.remaining;
    while (distance <= length + 1e-9) {
        const ratio = Math.min(1, distance / length);
        points.push({x: from.x + (to.x - from.x) * ratio, y: from.y + (to.y - from.y) * ratio});
        distance += spread;
    }
    stroke.remaining = distance - length;
    stroke.last = {...to};
    return points;
}
