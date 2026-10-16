import {textSize} from "../tools/text";
import {itemPathRibbon, type PathStyle} from "../tools/paths";

import {roofGeometry, type RoofKind} from "../tools/roofs";

export type Point = { x: number; y: number };
export type Color = [number, number, number, number];
export type WaterStyle = {
    deep_color: Color;
    shallow_color: Color;
    blend_distance: number;
    disable_border: boolean;
    smoothing?: number;
};
export type CaveStyle = { colorable?: boolean; ground_color: Color; wall_color: Color; blasts: Point[][] };
export type Kind =
    | RoofKind
    | {
    type: "room";
    rings: Point[][];
    floor: string | null;
    wall: string | null;
    width: number;
}
    | { type: "floor"; points: Point[]; color: Color }
    | {
    type: "wall" | "path";
    points: Point[];
    width: number;
    color: Color;
    blocks_sight?: boolean;
}
    | { type: "terrain_region"; rings: Point[][]; color: Color }
    | {
    type: "object";
    asset: string;
    position: Point;
    size: Point;
    rotation: number;
    pivot: Point;
    flip_x: boolean;
    flip_y: boolean;
}
    | { type: "portal"; bounds: [Point, Point]; closed: boolean; color: Color }
    | {
    type: "light";
    position: Point;
    range: number;
    intensity: number;
    smoothing?: number;
    color: Color;
    shadows: boolean;
}
    | { type: "text"; position: Point; text: string; size: number; color: Color };
export type TextStyle = {
    font: string;
    background: boolean;
    shape: "square" | "rounded";
    border_width: number;
    background_color: Color;
    border_color: Color;
};
export const defaultTextStyle = (): TextStyle => ({
    font: "", background: false, shape: "rounded",
    border_width: 1, background_color: [255, 255, 255, 255], border_color: [0, 0, 0, 255]
});
export type Item = {
    prefab?: { size: Point; items: Item[] } | null;
    id: string;
    name: string;
    locked: boolean;
    kind: Kind;
    blocks_light?: boolean;
    text_style?: TextStyle | null;
    path_style?: PathStyle | null;
    path_profile?: { widths: number[]; alpha: number[] } | null;
    portal_type?: "door" | "window" | null;
    texture: string | null;
    texture_scale: number;
    tint: Color;
    floor_tint?: Color;
    wall_tint?: Color;
    border: string | null;
    wall_attachment: { wall: string; segment: number; t: number; length: number } | null;
    water?: WaterStyle | null;
    cave?: CaveStyle | null;
    water_source_rings?: Point[][] | null;
    floor_holes?: Point[][];
    floor_outline?: boolean | null;
    wall_exclusions?: number[];
};

export function portalType(item: Item, asset?: AssetMeta): "door" | "window" {
    if (item.portal_type) return item.portal_type;
    const filename = (value: string) => value.split(/[\\/]/).at(-1) ?? "";
    const detect = (values: string[]): "door" | "window" | undefined => {
        const lower = values.map(value => value.toLowerCase());
        if (lower.some(value => value.includes("door") || value.includes("двер"))) return "door";
        if (lower.some(value => value.includes("window") || value.split(/[^\p{L}\p{N}]+/u)
            .some(part => ["окно", "окна"].includes(part)))) return "window";
    };
    return (asset && (detect([filename(asset.id), asset.name]) ?? detect(asset.tags)))
        ?? detect([filename(item.texture ?? ""), item.name])
        ?? (item.kind.type === "portal" && !item.kind.closed ? "window" : "door");
}

export function setPortalStyle(item: Item, asset: AssetMeta, previousAsset?: AssetMeta) {
    if (item.kind.type !== "portal") return;
    // Legacy portals infer light blocking from their texture. Freeze that
    // behavior before replacing the visual, even with a door/window asset.
    item.portal_type = portalType(item, previousAsset);
    item.texture = asset.id;
}

export function multiplyColors(a: Color, b: Color): Color {
    return a.map((value, i) => Math.floor((value * b[i] + 127) / 255)) as Color;
}

export function roomTint(item: Item, floor: boolean): Color {
    return multiplyColors(
        (floor ? item.floor_tint : item.wall_tint) ?? [255, 255, 255, 255],
        item.tint,
    );
}

export function setRoomTint(item: Item, floor: boolean, color: Color) {
    if (item.kind.type !== "room") return;
    item.floor_tint = roomTint(item, true);
    item.wall_tint = roomTint(item, false);
    item.tint = [255, 255, 255, 255];
    if (floor) item.floor_tint = color;
    else item.wall_tint = color;
}

export type Layer = {
    id: string;
    name: string;
    visible: boolean;
    locked: boolean;
    opacity: number;
    items: Item[];
};
export type Level = { id: string; name: string; layers: Layer[] };
export type AssetMeta = {
    id: string;
    name: string;
    path: string;
    tags: string[];
    size: Point;
    pivot: Point;
    sha256: string;
    color_mode: "none" | "tint" | "red_mask";
};
export type Asset = { meta: AssetMeta; category: string; builtin: boolean };
export type AssetLookup = Readonly<Record<string, AssetMeta>>;
export type Project = {
    format: string;
    version: number;
    id: string;
    name: string;
    width: number;
    height: number;
    pixels_per_grid: number;
    background: Color;
    ambient_light?: Color;
    background_asset: string | null;
    use_default_assets: boolean;
    levels: Level[];
    main_level?: string | null;
    active_variation?: string | null;
    variations?: Variation[];
    assets: Record<string, AssetMeta>;
    packs: { name: string; path: string }[];
    asset_catalogs: { id: string; name: string; packs?: string[] | null }[];
    external_assets: Record<string, string>;
    extensions: Record<string, unknown>;
};
export type Variation = {
    id: string; name: string; levels: Level[]; main_level?: string | null;
    background: Color; ambient_light: Color; background_asset: string | null;
};
export type Preferences = {
    version: number;
    dark: boolean;
    theme?: string | null;
    theme_variant?: string | null;
    accent: [number, number, number];
    locale: string;
    default_assets: boolean;
    asset_directories: { id: string; name: string; path: string }[];
    map_size_presets?: MapSizePreset[];
};
export type MapSizePreset = { id: string; name: string; width: number; height: number };
export type Locale = {
    id: string;
    name: string;
    strings: Record<string, string>;
};
export type AssetCatalog = {
    directory: Preferences["asset_directories"][number];
    assets: number;
    loose_images: number;
    warnings: string[];
    packs: {
        key: string;
        path: string;
        name: string;
        file: string;
        assets: number;
        metadata: {
            id: string;
            author: string;
            version: string;
            description: string;
            website: string;
            logo: string | null
        };
    }[];
};

export function displayDirectoryPath(path: string): string {
    if (path.startsWith("\\\\?\\UNC\\")) return "\\\\" + path.slice(8);
    if (/^\\\\\?\\[a-z]:\\/i.test(path)) return path.slice(4);
    return path;
}

export function duplicatePackFiles(catalogs: AssetCatalog[]): string[][] {
    const groups = new Map<string, Set<string>>();
    for (const catalog of catalogs) {
        for (const pack of catalog.packs) {
            if (!pack.metadata.id) continue;
            const paths = groups.get(pack.metadata.id) ?? new Set<string>();
            paths.add(displayDirectoryPath(pack.path));
            groups.set(pack.metadata.id, paths);
        }
    }
    return [...groups.values()].filter(paths => paths.size > 1).map(paths => [...paths]);
}

export function catalogPackEnabled(project: Project, catalog: AssetCatalog, key: string): boolean {
    const selected = project.asset_catalogs.find(ref => ref.id === catalog.directory.id);
    return !!selected && (selected.packs == null || selected.packs.includes(key));
}

function catalogKeys(catalog: AssetCatalog): string[] {
    return [...catalog.packs.map(pack => pack.key), ...(catalog.loose_images ? [""] : [])];
}

export function catalogSelectionState(project: Project, catalog: AssetCatalog): "none" | "partial" | "all" {
    const selected = project.asset_catalogs.find(ref => ref.id === catalog.directory.id);
    if (!selected) return "none";
    if (selected.packs == null) return "all";
    const keys = catalogKeys(catalog), enabled = new Set(selected.packs);
    if (keys.length && keys.every(key => enabled.has(key))) return "all";
    return enabled.size ? "partial" : "none";
}

export function setCatalogEnabled(project: Project, catalogs: AssetCatalog[], catalog: AssetCatalog, enabled: boolean): void {
    const selectedCatalog = project.asset_catalogs.find(ref => ref.id === catalog.directory.id);
    if (!enabled) {
        project.asset_catalogs = project.asset_catalogs.filter(ref => ref.id !== catalog.directory.id);
    } else if (selectedCatalog) {
        selectedCatalog.packs = null;
    } else {
        project.asset_catalogs.push({id: catalog.directory.id, name: catalog.directory.name, packs: null});
    }
    const ids = new Set(catalog.packs.map(pack => pack.metadata.id).filter(Boolean));
    for (const candidate of catalogs) {
        if (candidate.directory.id === catalog.directory.id) continue;
        const shared = candidate.packs.filter(pack => ids.has(pack.metadata.id)).map(pack => pack.key);
        if (!shared.length) continue;
        const selected = project.asset_catalogs.find(ref => ref.id === candidate.directory.id);
        if (enabled && selected && selected.packs == null) continue;
        const packs = new Set(selected ? selected.packs ?? catalogKeys(candidate) : []);
        for (const key of shared) {
            if (enabled) packs.add(key);
            else packs.delete(key);
        }
        if (!packs.size) {
            project.asset_catalogs = project.asset_catalogs.filter(ref => ref.id !== candidate.directory.id);
        } else if (selected) {
            selected.packs = [...packs];
        } else {
            project.asset_catalogs.push({
                id: candidate.directory.id,
                name: candidate.directory.name,
                packs: [...packs]
            });
        }
    }
}

export function setCatalogPack(project: Project, catalogs: AssetCatalog[], catalog: AssetCatalog, key: string, enabled: boolean): void {
    const id = catalog.packs.find(pack => pack.key === key)?.metadata.id;
    if (!id) {
        setSingleCatalogPack(project, catalog, key, enabled);
        return;
    }
    for (const candidate of catalogs) {
        for (const pack of candidate.packs) {
            if (pack.metadata.id === id) setSingleCatalogPack(project, candidate, pack.key, enabled);
        }
    }
}

function setSingleCatalogPack(project: Project, catalog: AssetCatalog, key: string, enabled: boolean): void {
    const selected = project.asset_catalogs.find(ref => ref.id === catalog.directory.id);
    const keys = selected ? selected.packs ?? catalogKeys(catalog) : [];
    const packs = keys.filter(value => value !== key);
    if (enabled) packs.push(key);
    if (!packs.length) {
        project.asset_catalogs = project.asset_catalogs.filter(ref => ref.id !== catalog.directory.id);
    } else if (selected) {
        selected.packs = packs;
    } else {
        project.asset_catalogs.push({id: catalog.directory.id, name: catalog.directory.name, packs});
    }
}

export type Snapshot = {
    prefabs?: import("./prefabs").PrefabEntry[];
    recovery?: { path: string; name: string; modified_ms: number } | null;
    project_open: boolean;
    project: Project;
    revision: number;
    dirty: boolean;
    path: string | null;
    undo: boolean;
    redo: boolean;
    assets: Asset[] | null;
    warnings: string[];
    preferences: Preferences;
    locales?: Locale[];
    catalogs: AssetCatalog[] | null;
};
export type Tool =
    | "select"
    | "pan"
    | "room"
    | "wall"
    | "floor"
    | "roof"
    | "path"
    | "terrain"
    | "cave"
    | "object"
    | "scatter"
    | "prefab"
    | "portal"
    | "light"
    | "text"
    | "erase";
export const uid = () => crypto.randomUUID();
export const newLayer = (name: string): Layer => ({
    id: uid(),
    name,
    visible: true,
    locked: false,
    opacity: 1,
    items: [],
});
export const newLevel = (name: string): Level => ({
    id: uid(),
    name,
    layers: ["Ground", "Architecture", "Objects", "Lighting"].map(newLayer),
});

export function newItem(
    name: string,
    kind: Kind,
    texture: string | null = null,
): Item {
    return {
        id: uid(),
        name,
        locked: false,
        kind,
        texture,
        texture_scale: 1,
        tint: [255, 255, 255, 255],
        border: null,
        wall_attachment: null,
    };
}

export function points(i: Item): Point[] {
    const k = i.kind;
    return k.type === "room" || k.type === "terrain_region"
        ? k.rings.flat()
        : "points" in k
            ? k.points
            : k.type === "portal"
                ? k.bounds
                : [k.position];
}

/** World geometry of a single prefab instance; these parts keep their parent ID scope. */
export function prefabParts(item: Item): Item[] {
    const contents = item.prefab, k = item.kind;
    if (!contents || k.type !== "object") return [item];
    const scale = k.size.x / contents.size.x, sx = k.flip_x ? -1 : 1, sy = k.flip_y ? -1 : 1;
    const sin = Math.sin(k.rotation), cos = Math.cos(k.rotation);
    const at = (p: Point): Point => ({
        x: k.position.x + (p.x * sx * cos - p.y * sy * sin) * scale,
        y: k.position.y + (p.x * sx * sin + p.y * sy * cos) * scale
    });
    return contents.items.map((source, index) => {
        const child = structuredClone(source), c = child.kind;
        child.id = `${item.id}:part:${index}`;
        child.tint = multiplyColors(child.tint, item.tint);
        child.blocks_light ||= item.blocks_light;
        if (c.type === "object") {
            c.position = at(c.position);
            c.size.x *= scale;
            c.size.y *= scale;
            c.rotation = k.rotation + c.rotation * sx * sy;
            if (k.flip_x) {
                c.pivot.x = 1 - c.pivot.x;
                c.flip_x = !c.flip_x;
            }
            if (k.flip_y) {
                c.pivot.y = 1 - c.pivot.y;
                c.flip_y = !c.flip_y;
            }
        } else if (c.type === "path" || c.type === "floor") {
            c.points = c.points.map(at);
            if (c.type === "path") c.width *= scale;
            child.texture_scale *= scale;
            child.floor_holes = child.floor_holes?.map(ring => ring.map(at));
        }
        return child;
    });
}

export function setObjectSize(item: Item, axis: "x" | "y", value: number) {
    if (item.kind.type !== "object") return;
    if (item.prefab) {
        const other = axis === "x" ? "y" : "x";
        item.kind.size[other] *= value / item.kind.size[axis];
    }
    item.kind.size[axis] = value;
}

export function corners(i: Item, assets: AssetLookup = {}): Point[] {
    const k = i.kind;
    if (k.type === "roof") return roofGeometry(k).outline;
    if (k.type === "path") return itemPathRibbon(i).map(v => v.position);
    if (k.type === "text") {
        const size = textSize(i);
        return [{x: k.position.x - size.x / 2, y: k.position.y - size.y / 2},
            {x: k.position.x + size.x / 2, y: k.position.y + size.y / 2}];
    }
    const portalSize = k.type === "portal" && i.texture ? assets[i.texture]?.size : undefined;
    if (k.type === "object" || (k.type === "portal" && portalSize && portalSize.x > 0 && portalSize.y > 0)) {
        // Portals render as a centered sprite whose width is the opening length.
        // The wall attachment segment remains separate from its visual bounds.
        const position = k.type === "object" ? k.position : {
            x: (k.bounds[0].x + k.bounds[1].x) / 2,
            y: (k.bounds[0].y + k.bounds[1].y) / 2,
        };
        const length = k.type === "portal" ? Math.hypot(k.bounds[1].x - k.bounds[0].x, k.bounds[1].y - k.bounds[0].y) : 0;
        const size = k.type === "object" ? k.size : {x: length, y: length * portalSize!.y / portalSize!.x};
        const pivot = k.type === "object" ? k.pivot : {x: 0.5, y: 0.5};
        const rotation = k.type === "object" ? k.rotation : Math.atan2(k.bounds[1].y - k.bounds[0].y, k.bounds[1].x - k.bounds[0].x);
        const c = Math.cos(rotation),
            s = Math.sin(rotation);
        return [
            [0, 0],
            [1, 0],
            [1, 1],
            [0, 1],
        ].map(([x, y]) => {
            x = (x - pivot.x) * size.x;
            y = (y - pivot.y) * size.y;
            return {
                x: position.x + x * c - y * s,
                y: position.y + x * s + y * c,
            };
        });
    }
    if (k.type === "light") {
        const r = k.range;
        return [
            {x: k.position.x - r, y: k.position.y - r},
            {x: k.position.x + r, y: k.position.y + r},
        ];
    }
    return points(i);
}

export function bounds(i: Item, assets: AssetLookup = {}) {
    const p = corners(i, assets);
    return {
        x: Math.min(...p.map((p) => p.x)),
        y: Math.min(...p.map((p) => p.y)),
        right: Math.max(...p.map((p) => p.x)),
        bottom: Math.max(...p.map((p) => p.y)),
    };
}

function inside(poly: Point[], p: Point) {
    let yes = false;
    for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
        const a = poly[i],
            b = poly[j];
        if (
            a.y > p.y !== b.y > p.y &&
            p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x
        )
            yes = !yes;
    }
    return yes;
}

export function segmentDistance(p: Point, a: Point, b: Point) {
    const dx = b.x - a.x,
        dy = b.y - a.y;
    const t = Math.max(
        0,
        Math.min(
            1,
            ((p.x - a.x) * dx + (p.y - a.y) * dy) / (dx * dx + dy * dy || 1),
        ),
    );
    return Math.hypot(p.x - a.x - t * dx, p.y - a.y - t * dy);
}

/** Geometry query for editing; canvas pointer selection uses GpuRenderer.pick for rendered alpha. */
export function hit(i: Item, p: Point, tolerance: number, assets: AssetLookup = {}): boolean {
    if (i.prefab) return prefabParts(i).some(part => hit(part, p, tolerance, assets));
    const k = i.kind;
    if (k.type === "roof") return inside(roofGeometry(k).outline, p);
    if (k.type === "room" || k.type === "terrain_region")
        return k.rings.reduce((v, r) => v !== inside(r, p), false)
            && !(i.cave?.blasts ?? []).reduce((v, r) => v !== inside(r, p), false);
    if (k.type === "floor") return inside(k.points, p) && !(i.floor_holes ?? []).some(r => inside(r, p));
    if (k.type === "path") {
        const ribbon = itemPathRibbon(i);
        for (let n = 2; n < ribbon.length; n += 2) {
            const quad = [ribbon[n - 2].position, ribbon[n].position, ribbon[n + 1].position, ribbon[n - 1].position];
            if (inside(quad, p) || quad.some((a, j) => segmentDistance(p, a, quad[(j + 1) % 4]) <= tolerance)) return true;
        }
        return false;
    }
    if (k.type === "wall")
        return k.points
            .slice(1)
            .some(
                (b, n) =>
                    segmentDistance(p, k.points[n], b) < Math.max(tolerance, k.width / 2),
            );
    if (k.type === "portal") {
        const quad = corners(i, assets);
        if (quad.length === 4)
            return inside(quad, p) || quad.some((a, n) => segmentDistance(p, a, quad[(n + 1) % 4]) <= tolerance);
        return segmentDistance(p, ...k.bounds) < tolerance + 0.1;
    }
    if (k.type === "object") return inside(corners(i), p);
    if (k.type === "light")
        return Math.hypot(p.x - k.position.x, p.y - k.position.y) <= tolerance + Math.max(0.3, k.range);
    if (k.type === "text") {
        const b = bounds(i);
        return p.x >= b.x - tolerance && p.x <= b.right + tolerance && p.y >= b.y - tolerance && p.y <= b.bottom + tolerance;
    }
    return false;
}

export function pick(level: Level, p: Point, tolerance: number, assets: AssetLookup = {}, layerId?: string) {
    const scope = layerId === undefined ? level : {...level, layers: level.layers.filter(l => l.id === layerId)};
    return paintOrder(scope).reverse()
        .find((i) => hit(i, p, tolerance, assets));
}

/** Paths can be edited through objects and floors, on any visible layer. */
export function pickPath(level: Level, p: Point, tolerance: number) {
    return pick({
        ...level,
        layers: level.layers.map(l => ({...l, items: l.items.filter(i => i.kind.type === "path")}))
    }, p, tolerance);
}

export function boxSelection(layer: Layer, a: Point, b: Point, assets: AssetLookup = {}, previous: string[] = []) {
    if (!layer.visible) return [];
    const available = new Set(layer.items.map(i => i.id));
    const ids = layer.items.filter(i => {
        const q = bounds(i, assets);
        return q.right >= Math.min(a.x, b.x) && q.x <= Math.max(a.x, b.x)
            && q.bottom >= Math.min(a.y, b.y) && q.y <= Math.max(a.y, b.y);
    }).map(i => i.id);
    return [...new Set([...previous.filter(id => available.has(id)), ...ids])];
}

export function paintOrder(level: Level): Item[] {
    const items = level.layers.filter(l => l.visible).flatMap(l => l.items);
    // Keep landscape below architecture regardless of layer or creation order.
    const pass = (i: Item) => i.cave ? 2 : i.water ? 1 : i.kind.type === "terrain_region" ? 0 : 3;
    items.sort((a, b) => pass(a) - pass(b));
    return items;
}

// Array order is back to front within each landscape/architecture pass.
export function moveToLayer(level: Level, ids: string[], targetId: string) {
    const target = level.layers.find(l => l.id === targetId);
    if (!target || target.locked || !target.visible) return;
    const moved: Item[] = [];
    for (const layer of level.layers) {
        if (layer === target || layer.locked) continue;
        layer.items = layer.items.filter(item => {
            if (item.locked || !ids.includes(item.id)) return true;
            moved.push(item);
            return false;
        });
    }
    target.items.push(...moved);
}

export type StackOrder = "up" | "down" | "front" | "back";

export function reorderItems(level: Level, ids: string[], order: StackOrder) {
    for (const layer of level.layers) {
        if (layer.locked) continue;
        const chosen = (i: Item) => !i.locked && ids.includes(i.id);
        const items = layer.items;
        if (order === "front" || order === "back") {
            const selected = items.filter(chosen), rest = items.filter(i => !chosen(i));
            layer.items = order === "front" ? [...rest, ...selected] : [...selected, ...rest];
        } else if (order === "up") {
            for (let i = items.length - 2; i >= 0; i--)
                if (chosen(items[i]) && !chosen(items[i + 1]))
                    [items[i], items[i + 1]] = [items[i + 1], items[i]];
        } else {
            for (let i = 1; i < items.length; i++)
                if (chosen(items[i]) && !chosen(items[i - 1]))
                    [items[i], items[i - 1]] = [items[i - 1], items[i]];
        }
    }
}

export function translate(item: Item, d: Point) {
    const k = item.kind;
    const shift = (p: Point) => {
        p.x += d.x;
        p.y += d.y;
    };
    item.water_source_rings?.flat().forEach(shift);
    item.cave?.blasts.flat().forEach(shift);
    item.floor_holes?.flat().forEach(shift);
    if (k.type === "room" || k.type === "terrain_region") k.rings.flat().forEach(shift);
    else if ("points" in k) k.points.forEach(shift);
    else if (k.type === "portal") k.bounds.forEach(shift);
    else shift(k.position);
}

export const hex = (c: Color) =>
    "#" +
    c
        .slice(0, 3)
        .map((v) => v.toString(16).padStart(2, "0"))
        .join("");
export const rgba = (h: string): Color => [
    parseInt(h.slice(1, 3), 16),
    parseInt(h.slice(3, 5), 16),
    parseInt(h.slice(5, 7), 16),
    255,
];
