import {type Item, type Layer, type Level, newItem, type Point, type WaterStyle} from "../map/model";
import {smoothWaterContours, terrainContours, terrainRegionsTouch, type WaterShape} from "./terrain";

export {terrainRegionsTouch as waterRegionsTouch} from "./terrain";

export function setWaterStyle(item: Item, style: WaterStyle) {
    if (!item.water || item.kind.type !== "terrain_region") return;
    item.water_source_rings ??= structuredClone(item.kind.rings);
    item.water = {...structuredClone(style), smoothing: style.smoothing ?? 0.2};
    item.kind.rings = smoothWaterContours(structuredClone(item.water_source_rings), item.water.smoothing!, 2);
}

// Touching waters form one body across layers. The last brush supplies the
// material for the entire connected body, while locked water stays protected.
export function paintWaterOnLevel(level: Level, target: Layer, trail: Point[], radius: number, style: WaterStyle,
                                  shape: WaterShape, name: string, polygon?: Point[]) {
    const fresh = terrainContours(polygon ? [] : trail, radius, {shape, polygon, smoothing: 0});
    if (!fresh.length || target.locked || !target.visible) return;
    const groups = [fresh];
    const joined: { layer: Layer; item: Item }[] = [];
    let changed = true;
    while (changed) {
        changed = false;
        for (const layer of level.layers.filter(l => l.visible)) for (const item of layer.items) {
            if (!item.water || item.kind.type !== "terrain_region" || joined.some(entry => entry.item === item)) continue;
            const rings = item.kind.rings;
            if (!groups.some(group => terrainRegionsTouch(group, rings))) continue;
            if (item.locked || layer.locked) return null;
            joined.push({layer, item});
            groups.push(item.water_source_rings ?? rings);
            changed = true;
        }
    }
    const rings = terrainContours([], radius, {shape, regions: groups, smoothing: 0});
    if (!rings.length) return;
    const item = joined[0]?.item ?? newItem(name, {type: "terrain_region", rings, color: [255, 255, 255, 255]});
    item.kind = {type: "terrain_region", rings, color: [255, 255, 255, 255]};
    item.water_source_rings = rings;
    item.water = {...structuredClone(style), smoothing: style.smoothing ?? 0.2};
    setWaterStyle(item, item.water);
    item.tint = [255, 255, 255, 255];
    item.texture = null;
    item.border = null;
    for (const entry of joined.slice(1)) entry.layer.items = entry.layer.items.filter(i => i !== entry.item);
    if (!joined.length) target.items.push(item);
    return item;
}

export function paintWater(items: Item[], trail: Point[], radius: number, style: WaterStyle,
                           shape: WaterShape, name: string, polygon?: Point[]) {
    const layer: Layer = {id: "", name: "", visible: true, locked: false, opacity: 1, items};
    const item = paintWaterOnLevel({
        id: "",
        name: "",
        layers: [layer]
    }, layer, trail, radius, style, shape, name, polygon);
    if (layer.items !== items) items.splice(0, items.length, ...layer.items);
    return item;
}
