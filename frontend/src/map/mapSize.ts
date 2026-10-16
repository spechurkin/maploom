import {type MapSizePreset, type Project, translate} from "./model";
import {syncActiveVariation} from "./variations";

export type MapEdges = { top: number; right: number; bottom: number; left: number };
export const zeroEdges = (): MapEdges => ({top: 0, right: 0, bottom: 0, left: 0});
export const validMapSize = (width: number, height: number) =>
    [width, height].every(n => Number.isInteger(n) && n >= 1 && n <= 512);

export function resizedMapSize(project: Pick<Project, "width" | "height">, edges: MapEdges) {
    return {width: project.width + edges.left + edges.right, height: project.height + edges.top + edges.bottom};
}

export function resizeMap(project: Project, edges: MapEdges): Project {
    const size = resizedMapSize(project, edges);
    if (!Object.values(edges).every(Number.isInteger) || !validMapSize(size.width, size.height))
        throw Error("Map dimensions must be 1–512 grid cells");
    const next = structuredClone(project);
    Object.assign(next, size);
    for (const level of next.levels) for (const layer of level.layers) for (const item of layer.items)
        translate(item, {x: edges.left, y: edges.top});
    for (const variation of next.variations ?? []) {
        if (variation.id === next.active_variation) continue;
        for (const level of variation.levels) for (const layer of level.layers) for (const item of layer.items)
            translate(item, {x: edges.left, y: edges.top});
    }
    syncActiveVariation(next);
    return next;
}

export const builtInMapSizes: MapSizePreset[] = [
    {id: "tv40", name: '40" TV', width: 35, height: 20},
    {id: "tv55", name: '55" TV', width: 48, height: 27},
    {id: "a3", name: "A3 Paper", width: 16, height: 11},
    {id: "a4", name: "A4 Paper", width: 11, height: 8},
];
