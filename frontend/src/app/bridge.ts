import {isTauri} from "@tauri-apps/api/core";
import {invoke} from "./native";
import {type GpuScene, ingestGpuScene, knownGpuMeshes, knownGpuTextures, previewGpuScene,} from "../rendering/gpu";
import {type AssetMeta, type Item, newItem, newLevel, Point, Project, Snapshot, translate, uid,} from "../map/model";

import {type MapEdges, resizeMap, validMapSize} from "../map/mapSize";
import {protectMainLevel} from "../map/levels";
import {addVariation, removeVariation, renameVariation, selectVariation, syncActiveVariation} from "../map/variations";
import {capturePrefab, prefabInstance} from "../map/prefabs";

export const desktop = isTauri();
// Set by the desktop host before React starts, without waiting for catalogs.
export const startupDark = (globalThis as typeof globalThis & { __MAPLOOM_DARK__?: boolean }).__MAPLOOM_DARK__ ?? true;
let fontList: Promise<string[]> | undefined;

export function localFonts(): Promise<string[]> {
    return fontList ??= desktop ? invoke<string[]>("editor_fonts").catch(error => {
        fontList = undefined;
        throw error;
    }) : Promise.resolve([]);
}

export type Action = { type: string; [key: string]: unknown };

export async function snapshot(): Promise<Snapshot> {
    if (desktop) return invoke("editor_snapshot");
    return preview;
}

export async function act(action: Action, revision: number): Promise<Snapshot> {
    if (desktop) return invoke("editor_action", {action, revision});
    if (action.type === "load_library") return structuredClone(preview);
    if (action.type === "save_prefab") {
        const ids = action.ids as string[];
        const items = preview.project.levels[Number(action.level)].layers.filter(l => l.visible)
            .flatMap(l => l.items.filter(i => ids.includes(i.id)).map(source => {
                const item = structuredClone(source);
                item.tint[3] = Math.round(item.tint[3] * l.opacity);
                return item;
            }));
        const prefab = capturePrefab(String(action.name), items);
        preview.prefabs = [...preview.prefabs ?? [], {key: `user:${prefab.id}`, source: "user", prefab}];
        return structuredClone(preview);
    }
    if (action.type === "delete_prefab") {
        preview.prefabs = preview.prefabs?.filter(p => p.source !== "user" || p.prefab.id !== action.id);
        return structuredClone(preview);
    }
    if (action.type === "place_prefab") {
        const entry = preview.prefabs?.find(p => p.key === action.key);
        if (!entry) throw Error("Choose prefab");
        const target = preview.project.levels[Number(action.level)].layers[Number(action.layer)];
        if (!target.visible || target.locked) throw Error("Layer is not editable");
        past.push(structuredClone(preview.project));
        future = [];
        target.items.push(prefabInstance(entry.prefab, action.position as Point, Number(action.scale), Number(action.rotation)));
        syncActiveVariation(preview.project);
        preview.revision++;
        preview.dirty = true;
        preview.undo = true;
        preview.redo = false;
        return structuredClone(preview);
    }
    if (["add_variation", "select_variation", "remove_variation", "rename_variation"].includes(action.type)) {
        const next = structuredClone(preview.project);
        if (action.type === "add_variation") addVariation(next, String(action.name));
        if (action.type === "select_variation") selectVariation(next, String(action.id));
        if (action.type === "remove_variation") removeVariation(next);
        if (action.type === "rename_variation") renameVariation(next, String(action.name));
        if (JSON.stringify(next) !== JSON.stringify(preview.project)) {
            past.push(structuredClone(preview.project));
            future = [];
            preview.project = next;
        }
    } else if (action.type === "resize_map") {
        const {top, right, bottom, left} = action as unknown as MapEdges;
        const next = resizeMap(preview.project, {top, right, bottom, left});
        if (JSON.stringify(next) !== JSON.stringify(preview.project)) {
            past.push(structuredClone(preview.project));
            future = [];
            preview.project = next;
        }
    } else if (action.type === "commit") {
        const next = action.project as Project;
        if (next.active_variation !== preview.project.active_variation ||
            JSON.stringify(next.variations) !== JSON.stringify(preview.project.variations))
            throw Error("Use variation actions to change variations");
        protectMainLevel(preview.project, action.project as Project);
        past.push(structuredClone(preview.project));
        future = [];
        preview.project = action.project as Project;
    } else if (action.type === "new") {
        if (!validMapSize(Number(action.width), Number(action.height))) throw Error("Map dimensions must be 1–512 grid cells");
        preview.project_open = true;
        past = [];
        future = [];
        preview.project = makePreview(action.demo === true);
        preview.project.name = String(action.name);
        preview.project.width = Number(action.width);
        preview.project.height = Number(action.height);
        saved = structuredClone(preview.project);
    } else if (action.type === "undo") {
        if (past.length) {
            future.push(preview.project);
            preview.project = past.pop()!;
        }
    } else if (action.type === "redo") {
        if (future.length) {
            past.push(preview.project);
            preview.project = future.pop()!;
        }
    } else if (action.type === "preferences") {
        preview.preferences = action.preferences as Snapshot["preferences"];
        if (typeof localStorage !== "undefined") localStorage.setItem("maploom:preview-presets", JSON.stringify(preview.preferences.map_size_presets ?? []));
    } else if (["add", "move", "delete", "duplicate"].includes(action.type)) {
        past.push(structuredClone(preview.project));
        future = [];
        const level = preview.project.levels[Number(action.level)];
        const ids = action.ids as string[];
        if (action.type === "add")
            level.layers[Number(action.layer)].items.push(
                action.item as ReturnType<typeof newItem>,
            );
        for (const l of level.layers.filter((l) => !l.locked)) {
            if (action.type === "delete")
                l.items = l.items.filter((i) => i.locked || !ids.includes(i.id));
            if (action.type === "move")
                l.items
                    .filter((i) => !i.locked && ids.includes(i.id))
                    .forEach((i) => translate(i, action.delta as Point));
            if (action.type === "duplicate")
                l.items.push(
                    ...l.items
                        .filter((i) => !i.locked && ids.includes(i.id))
                        .map((i) => {
                            i = structuredClone(i);
                            i.id = uid();
                            translate(i, {x: 0.5, y: 0.5});
                            return i;
                        }),
                );
        }
    } else if (action.type === "autosave") return structuredClone(preview);
    else throw Error("Preview hint");
    syncActiveVariation(preview.project);
    preview.revision++;
    preview.dirty = JSON.stringify(saved) !== JSON.stringify(preview.project);
    preview.undo = past.length > 0;
    preview.redo = future.length > 0;
    return structuredClone(preview);
}

export async function scene(
    level: number,
    grid: boolean,
    lights: boolean,
): Promise<GpuScene> {
    const scene = desktop
        ? await invoke<GpuScene>("editor_scene", {
            level,
            grid,
            lights,
            knownTextures: knownGpuTextures(),
            knownMeshes: knownGpuMeshes(),
        })
        : previewGpuScene(preview.project, level, preview.revision, lights);
    ingestGpuScene(scene);
    return scene;
}

export async function thumbnail(id: string, color?: number[], maxSide?: number): Promise<string> {
    if (!desktop) return "";
    return invoke("asset_thumbnail", {id, color: color ?? null, maxSide: maxSide ?? null});
}

export type PortalPreviewRequest = {
    level: number; layer: number; position: Point; asset: string; color: number[];
    grid: boolean; lights: boolean; revision: number;
};

export async function portalPreview(request: PortalPreviewRequest): Promise<GpuScene> {
    if (!desktop) throw Error("Preview hint");
    const result = await invoke<GpuScene>("editor_portal_preview", {
        request, knownTextures: knownGpuTextures(), knownMeshes: knownGpuMeshes(),
    });
    ingestGpuScene(result);
    return result;
}

export type DrawingPreviewRequest = {
    level: number; layer: number; item: Item; under: boolean;
    grid: boolean; lights: boolean; revision: number;
};

export async function drawingPreview(request: DrawingPreviewRequest): Promise<GpuScene> {
    let result: GpuScene;
    if (desktop) result = await invoke<GpuScene>("editor_drawing_preview", {
        request, knownTextures: knownGpuTextures(), knownMeshes: knownGpuMeshes(),
    });
    else {
        const project = structuredClone(preview.project);
        const layer = project.levels[request.level].layers[request.layer];
        if (request.under) layer.items.unshift(request.item);
        else layer.items.push(request.item);
        // Each transient browser mesh needs its own cache key as the cursor moves.
        result = previewGpuScene(project, request.level, --drawingRevision, request.lights);
        result.revision = request.revision;
    }
    ingestGpuScene(result);
    return result;
}

let drawingRevision = 0;

const silhouettes = new Map<string, { promise: Promise<Point[]>; points?: Point[] }>();
const silhouetteKey = (asset: AssetMeta) => JSON.stringify([asset.id, asset.sha256]);

export function cachedSilhouette(asset: AssetMeta): Point[] | undefined {
    return silhouettes.get(silhouetteKey(asset))?.points;
}

export function assetSilhouette(asset: AssetMeta): Promise<Point[]> {
    const key = silhouetteKey(asset);
    const cached = silhouettes.get(key);
    if (cached) return cached.promise;
    const entry: { promise: Promise<Point[]>; points?: Point[] } = {
        // The browser preview has no source textures. Never substitute selection bounds.
        promise: desktop ? invoke<Point[]>("asset_silhouette", {id: asset.id}) : Promise.resolve([]),
    };
    entry.promise = entry.promise.then(points => {
        entry.points = points;
        return points;
    }, error => {
        silhouettes.delete(key);
        throw error;
    });
    if (silhouettes.size >= 128) silhouettes.delete(silhouettes.keys().next().value!);
    silhouettes.set(key, entry);
    return entry.promise;
}

function makePreview(demo = true): Project {
    const levels = [newLevel("Ground floor")];
    if (demo) {
        const room = newItem("Sanctuary", {
            type: "room",
            rings: [
                [
                    [7, 5],
                    [15, 5],
                    [15, 9],
                    [18, 9],
                    [18, 5],
                    [25, 5],
                    [25, 12],
                    [19, 12],
                    [19, 15],
                    [23, 15],
                    [23, 20],
                    [13, 20],
                    [13, 15],
                    [14, 15],
                    [14, 12],
                    [7, 12],
                ].map(([x, y]) => ({x, y})),
            ],
            floor: null,
            wall: null,
            width: 0.18,
        });
        levels[0].layers[1].items = [room];
        levels[0].layers[2].items = [
            newItem("Sanctuary", {
                type: "text",
                position: {x: 11, y: 8},
                text: "SANCTUARY",
                size: 0.35,
                color: [47, 48, 43, 255],
            }),
        ];
    }
    return {
        format: "maploom/project",
        version: 1,
        id: uid(),
        name: demo ? "The Mosslight Sanctuary" : "Untitled map",
        width: 32,
        height: 24,
        pixels_per_grid: 100,
        background: [101, 124, 90, 255],
        background_asset: null,
        use_default_assets: true,
        levels,
        assets: {},
        packs: [],
        asset_catalogs: [],
        external_assets: {},
        extensions: {},
    };
}

let past: Project[] = [],
    future: Project[] = [];
const preview: Snapshot = {
    prefabs: [],
    project_open: false,
    project: makePreview(false),
    revision: 0,
    dirty: false,
    path: null,
    undo: false,
    redo: false,
    assets: [],
    warnings: [],
    catalogs: [],
    preferences: {
        version: 1,
        dark: true,
        accent: [137, 160, 109],
        locale: "en",
        map_size_presets: previewPresets(),
        default_assets: true,
        asset_directories: [],
    },
};

let saved = structuredClone(preview.project);

function previewPresets(): NonNullable<Snapshot["preferences"]["map_size_presets"]> {
    try {
        return JSON.parse(localStorage.getItem("maploom:preview-presets") ?? "[]");
    } catch {
        return [];
    }
}
