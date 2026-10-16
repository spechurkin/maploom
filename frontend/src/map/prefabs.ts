import {bounds, type Item, newItem, type Point, prefabParts, translate, uid} from "./model";

export type Prefab = { id: string; name: string; items: Item[]; preview?: string | null };
export type PrefabEntry = { key: string; source: "default" | "user" | "pack"; prefab: Prefab };

export function canCreatePrefab(items: Item[]): boolean {
    return items.length >= 2 && items.length <= 1000 && items.every(i => i.kind.type === "object");
}

export function prefabBounds(items: Item[]) {
    const boxes = items.map(i => bounds(i));
    return {
        x: Math.min(...boxes.map(b => b.x)), y: Math.min(...boxes.map(b => b.y)),
        right: Math.max(...boxes.map(b => b.right)), bottom: Math.max(...boxes.map(b => b.bottom)),
    };
}

export function capturePrefab(name: string, items: Item[]): Prefab {
    if (!canCreatePrefab(items) || !name.trim() || name.trim().length > 80) throw Error("Invalid prefab selection");
    items = items.flatMap(prefabParts);
    if (items.length > 1000) throw Error("Invalid prefab selection");
    const b = prefabBounds(items);
    const center = {x: (b.x + b.right) / 2, y: (b.y + b.bottom) / 2};
    return {
        id: uid(), name: name.trim(), items: items.map(source => {
            const item = structuredClone(source);
            item.locked = false;
            item.wall_attachment = null;
            translate(item, {x: -center.x, y: -center.y});
            return item;
        })
    };
}

export function prefabInstance(prefab: Prefab, position: Point, scale = 1, rotation = 0): Item {
    if (!Number.isFinite(position.x) || !Number.isFinite(position.y) || !Number.isFinite(scale)
        || scale < 0.05 || scale > 100 || !Number.isFinite(rotation)) throw Error("Invalid prefab transform");
    const b = prefabBounds(prefab.items);
    const size = {
        x: Math.max(Math.abs(b.x), Math.abs(b.right)) * 2,
        y: Math.max(Math.abs(b.y), Math.abs(b.bottom)) * 2
    };
    const first = prefab.items.find(i => i.kind.type === "object");
    const asset = prefab.preview ?? (first?.kind.type === "object" ? first.kind.asset : prefab.items[0].texture ?? "");
    const item = newItem(prefab.name, {
        type: "object", asset, position: {...position}, size: {x: size.x * scale, y: size.y * scale},
        rotation, pivot: {x: 0.5, y: 0.5}, flip_x: false, flip_y: false
    });
    item.prefab = {size, items: structuredClone(prefab.items)};
    return item;
}

/** Same rigid composition transform used by the desktop placement command. */
export function placePrefab(prefab: Prefab, position: Point, scale = 1, rotation = 0, freshIds = true): Item[] {
    const sin = Math.sin(rotation), cos = Math.cos(rotation);
    return prefab.items.map(source => {
        const item = structuredClone(source);
        if (freshIds) item.id = uid();
        item.locked = false;
        item.wall_attachment = null;
        if (item.kind.type === "object") {
            const p = item.kind.position;
            item.kind.position = {
                x: position.x + (p.x * cos - p.y * sin) * scale,
                y: position.y + (p.x * sin + p.y * cos) * scale,
            };
            item.kind.size.x *= scale;
            item.kind.size.y *= scale;
            item.kind.rotation += rotation;
        } else if (item.kind.type === "path" || item.kind.type === "floor") {
            item.kind.points = item.kind.points.map(p => ({
                x: position.x + (p.x * cos - p.y * sin) * scale,
                y: position.y + (p.x * sin + p.y * cos) * scale,
            }));
            if (item.kind.type === "path") item.kind.width *= scale;
            item.texture_scale *= scale;
        }
        return item;
    });
}
