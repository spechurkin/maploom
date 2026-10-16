import {test} from "node:test";
import assert from "node:assert/strict";
import {paintWater, paintWaterOnLevel, setWaterStyle} from "../src/tools/water";
import {appendTerrainPoint, smoothWaterContours} from "../src/tools/terrain";
import {hit, type Item, newItem, newLevel, paintOrder, pick, translate, type WaterStyle} from "../src/map/model";

const style: WaterStyle = {
    deep_color: [55, 159, 154, 255], shallow_color: [141, 203, 177, 255],
    blend_distance: 1.5, disable_border: false, smoothing: 0.6
};

test("water extends as one selectable region, retains its style and moves with its shore", () => {
    const items: Item[] = [];
    paintWater(items, [{x: 4, y: 4}], 2, style, "circle", "Water");
    const id = items[0].id;
    const trail = [{x: 4, y: 4}];
    appendTerrainPoint(trail, {x: 10, y: 4}, 2);
    paintWater(items, trail, 2, style, "circle", "Water");
    assert.equal(items.length, 1);
    assert.equal(items[0].id, id);
    assert.equal(items[0].kind.type, "terrain_region");
    for (let x = 4; x <= 10; x += 0.25) assert.ok(hit(items[0], {x, y: 4}, 0));
    const copy = JSON.parse(JSON.stringify(items[0]));
    translate(copy, {x: 2, y: 5});
    assert.ok(hit(copy, {x: 12, y: 9}, 0));
    assert.deepEqual(copy.water, style);
    assert.equal(copy.texture, null);
});

test("square and polygon shapes fill their corners, circles do not", () => {
    const round: Item[] = [], square: Item[] = [], polygon: Item[] = [];
    paintWater(round, [{x: 4, y: 4}], 2, style, "circle", "Water");
    paintWater(square, [{x: 4, y: 4}], 2, style, "square", "Water");
    assert.equal(hit(round[0], {x: 5.7, y: 5.7}, 0), false);
    assert.ok(hit(square[0], {x: 5.7, y: 5.7}, 0));
    paintWater(polygon, [], 2, style, "polygon", "Water", [{x: 2, y: 2}, {x: 8, y: 2}, {x: 2, y: 8}]);
    assert.ok(hit(polygon[0], {x: 3, y: 3}, 0));
    assert.equal(hit(polygon[0], {x: 7, y: 7}, 0), false);
});

test("islands survive extending water, and remain dry and unselectable", () => {
    const items: Item[] = [];
    const ring = Array.from({length: 80}, (_, i) => ({
        x: 5 + 3 * Math.cos(i / 80 * Math.PI * 2),
        y: 5 + 3 * Math.sin(i / 80 * Math.PI * 2)
    }));
    paintWater(items, ring, 0.7, style, "circle", "Water");
    assert.equal(hit(items[0], {x: 5, y: 5}, 0), false);
    paintWater(items, [{x: 8, y: 5}, {x: 8.3, y: 5}], 0.7, style, "circle", "Water");
    assert.equal(items.length, 1);
    assert.equal(hit(items[0], {x: 5, y: 5}, 0), false);
    assert.ok(hit(items[0], {x: 8.3, y: 5}, 0));
});

test("new water recolors and unites every intersecting river across layers", () => {
    const level = newLevel("Map");
    paintWater(level.layers[0].items, [{x: 4, y: 4}], 2, style, "circle", "Water");
    paintWater(level.layers[2].items, [{x: 10, y: 4}], 2, {...style, disable_border: true}, "circle", "Water");
    const trail = [{x: 4, y: 4}];
    appendTerrainPoint(trail, {x: 10, y: 4}, 1);
    const latest: WaterStyle = {
        deep_color: [10, 40, 180, 255],
        shallow_color: [100, 180, 230, 255],
        blend_distance: 0.5,
        disable_border: true,
        smoothing: 0.6
    };
    paintWaterOnLevel(level, level.layers[1], trail, 1, latest, "circle", "Water");
    const waters = level.layers.flatMap(l => l.items).filter(i => i.water);
    assert.equal(waters.length, 1);
    assert.deepEqual(waters[0].water, latest);
    for (let x = 4; x <= 10; x += 0.25) assert.ok(hit(waters[0], {x, y: 4}, 0));
});

test("separate lakes and dry islands do not merge, while locked water blocks overlapping paint atomically", () => {
    const items: Item[] = [];
    paintWater(items, [{x: 4, y: 4}], 2, style, "circle", "Water");
    paintWater(items, [{x: 15, y: 4}], 2, style, "circle", "Water");
    assert.equal(items.length, 2);
    items[0].locked = true;
    const before = structuredClone(items);
    assert.equal(paintWater(items, [{x: 5, y: 4}], 2, {...style, disable_border: true}, "circle", "Water"), null);
    assert.deepEqual(items, before);
    const island: Item[] = [];
    const ring = Array.from({length: 80}, (_, i) => ({
        x: 5 + 3 * Math.cos(i / 80 * Math.PI * 2),
        y: 5 + 3 * Math.sin(i / 80 * Math.PI * 2)
    }));
    paintWater(island, ring, 0.5, style, "circle", "Water");
    paintWater(island, [{x: 5, y: 5}], 0.5, style, "circle", "Water");
    assert.equal(island.length, 2);
});

test("water merges past intervening objects without modifying them", () => {
    const items: Item[] = [];
    paintWater(items, [{x: 4, y: 4}], 2, style, "circle", "Water");
    const floor = newItem("Floor", {
        type: "floor",
        points: [{x: 0, y: 0}, {x: 1, y: 0}, {x: 1, y: 1}],
        color: [255, 255, 255, 255]
    });
    items.push(floor);
    paintWater(items, [{x: 5, y: 4}], 2, {...style, disable_border: true}, "circle", "Water");
    assert.equal(items.length, 2);
    assert.equal(items[1], floor);
    assert.equal(items[0].water?.disable_border, true);
});

test("smoothing rounds water corners, can be disabled, and keeps tiny outlines finite", () => {
    const square = [[{x: 0, y: 0}, {x: 4, y: 0}, {x: 4, y: 4}, {x: 0, y: 4}]];
    assert.deepEqual(smoothWaterContours(square, 0, 2), square);
    const smooth = smoothWaterContours(square, 1, 2);
    assert.ok(smooth[0].some(p => p.x > 0.01 && p.x < 0.3 && p.y > 0.01 && p.y < 0.3));
    const tiny = smoothWaterContours([[{x: 0, y: 0}, {x: 0.01, y: 0}, {x: 0, y: 0.01}]], 1, 2);
    assert.ok(tiny.flat().every(p => Number.isFinite(p.x) && Number.isFinite(p.y)));
});

test("existing water smoothing is reversible after saving, moving, and repeated edits", () => {
    const items: Item[] = [];
    paintWater(items, [{x: 4, y: 4}], 2, style, "square", "Water");
    const item = JSON.parse(JSON.stringify(items[0])) as Item;
    const source = structuredClone(item.water_source_rings);
    setWaterStyle(item, {...style, smoothing: 1});
    const smoothed = structuredClone(item.kind);
    setWaterStyle(item, {...style, smoothing: 0});
    assert.deepEqual(item.kind.type === "terrain_region" && item.kind.rings, source);
    setWaterStyle(item, {...style, smoothing: 1});
    assert.deepEqual(item.kind, smoothed);
    setWaterStyle(item, {...style, smoothing: 0});
    translate(item, {x: 3, y: 5});
    assert.deepEqual(item.kind.type === "terrain_region" && item.kind.rings, item.water_source_rings);
    assert.equal(item.water_source_rings![0][0].x, source![0][0].x + 3);
    setWaterStyle(item, {...style, smoothing: 1});
    assert.equal(hit(item, {x: 7, y: 9}, 0), true);
});

test("water stays above later terrain in every layer, with architecture above water", () => {
    const level = newLevel("Map");
    paintWater(level.layers[0].items, [{x: 4, y: 4}], 2, style, "circle", "Water");
    const water = level.layers[0].items[0];
    const terrain = newItem("Land", {
        type: "terrain_region",
        rings: [[{x: 0, y: 0}, {x: 8, y: 0}, {x: 8, y: 8}, {x: 0, y: 8}]],
        color: [255, 255, 255, 255]
    });
    level.layers[2].items.push(terrain);
    assert.deepEqual(paintOrder(level).map(i => i.id), [terrain.id, water.id]);
    assert.equal(pick(level, {x: 4, y: 4}, 0)?.id, water.id);
    const floor = newItem("Floor", {
        type: "floor",
        points: [{x: 3, y: 3}, {x: 5, y: 3}, {x: 5, y: 5}, {x: 3, y: 5}],
        color: [255, 255, 255, 255]
    });
    level.layers[1].items.push(floor);
    assert.equal(pick(level, {x: 4, y: 4}, 0)?.id, floor.id);
    level.layers[0].visible = false;
    assert.deepEqual(paintOrder(level).map(i => i.id), [terrain.id, floor.id]);
    assert.equal(pick(level, {x: 4, y: 4}, 0)?.id, floor.id);
});
