import {test} from "node:test";
import assert from "node:assert/strict";
import {addTerrainRegion, appendTerrainPoint, terrainContours, terrainRegionsTouch} from "../src/tools/terrain";
import {
    bounds,
    hit,
    type Item,
    newItem,
    newLayer,
    newLevel,
    paintOrder,
    pick,
    type Point,
    translate
} from "../src/map/model";

test("later terrain stays below floors and rooms in the same or a higher layer without water or caves", () => {
    const points = [{x: 0, y: 0}, {x: 8, y: 0}, {x: 8, y: 8}, {x: 0, y: 8}];
    for (const type of ["floor", "room"] as const) for (const terrainLayer of [1, 2]) {
        const level = newLevel("Map");
        const architecture = newItem(type, type === "floor"
            ? {type, points, color: [255, 255, 255, 255]}
            : {type, rings: [points], floor: null, wall: null, width: 0.2});
        const first = newItem("Land", {type: "terrain_region", rings: [points], color: [255, 255, 255, 255]});
        const last = structuredClone(first);
        last.id = "later-terrain";
        level.layers[1].items.push(architecture);
        level.layers[terrainLayer].items.push(first, last);
        const original = structuredClone(level);
        assert.deepEqual(paintOrder(level), [first, last, architecture]);
        assert.equal(pick(level, {x: 4, y: 4}, 0), architecture);
        assert.deepEqual(level, original);
        level.layers[terrainLayer].visible = false;
        assert.ok(!paintOrder(level).includes(first));
    }
});

test("a brush stroke is one selectable region with a shared outline", () => {
    const trail = Array.from({length: 21}, (_, i) => ({x: i * 0.2, y: 2}));
    const rings = terrainContours(trail, 1);
    assert.equal(rings.length, 1);
    const item = newItem("Terrain", {type: "terrain_region", rings, color: [255, 255, 255, 255]});
    for (const p of trail) assert.equal(hit(item, p, 0), true);
    assert.equal(hit(item, {x: 2, y: 4}, 0), false);
    const original = bounds(item);
    translate(item, {x: 3, y: 5});
    const moved = bounds(item);
    assert.equal(moved.x, original.x + 3);
    assert.equal(moved.bottom, original.bottom + 5);
    assert.equal(hit(item, {x: 5, y: 7}, 0), true);
    const stored = JSON.parse(JSON.stringify(item));
    assert.ok(stored.kind.rings.length);
    assert.equal("radius" in stored.kind, false);
    assert.equal("position" in stored.kind, false);
});

test("terrain outlines retain holes and disconnected regions", () => {
    const trail = Array.from({length: 80}, (_, i) => ({
        x: 5 + 3 * Math.cos(i / 80 * Math.PI * 2),
        y: 5 + 3 * Math.sin(i / 80 * Math.PI * 2),
    }));
    const rings = terrainContours(trail, 0.6);
    assert.equal(rings.length, 2);
    const item = newItem("Ring", {type: "terrain_region", rings, color: [255, 255, 255, 255]});
    assert.equal(hit(item, {x: 5, y: 5}, 0), false);
    assert.equal(hit(item, {x: 8, y: 5}, 0), true);
    assert.equal(terrainContours([{x: 0, y: 0}, {x: 5, y: 0}], 1).length, 2);
    assert.deepEqual(terrainContours([], 1), []);
});

test("fast pointer moves leave a continuous brush stroke", () => {
    const trail = [{x: 0, y: 2}];
    appendTerrainPoint(trail, {x: 10, y: 2}, 0.5);
    const rings = terrainContours(trail, 0.5);
    assert.equal(rings.length, 1);
    const item = newItem("Terrain", {type: "terrain_region", rings, color: [255, 255, 255, 255]});
    for (let x = 0; x <= 10; x += 0.25) assert.equal(hit(item, {x, y: 2}, 0), true);
});

function paint(x: number, y = 4, radius = 2) {
    const item = newItem("Terrain", {
        type: "terrain_region", rings: terrainContours([{x, y}], radius), color: [255, 255, 255, 255],
    }, "grass_tile.png");
    item.border = "grass_border.png";
    item.texture_scale = 4;
    return item;
}

test("overlapping terrain strokes have one selectable outline with no interior borders", () => {
    const layer = newLayer("Ground");
    const first = paint(4);
    addTerrainRegion(layer, first, 2);
    addTerrainRegion(layer, paint(6), 2);
    addTerrainRegion(layer, paint(8, 5), 2);
    assert.equal(layer.items.length, 1);
    assert.equal(layer.items[0].id, first.id);
    const item = layer.items[0];
    assert.equal(item.kind.type, "terrain_region");
    if (item.kind.type !== "terrain_region") return;
    assert.equal(item.kind.rings.length, 1);
    // An internal brush boundary would have filled points on both sides.
    for (const p of item.kind.rings[0]) {
        assert.ok(![{x: 4, y: 4}, {x: 6, y: 4}, {x: 8, y: 5}].some(c => Math.hypot(p.x - c.x, p.y - c.y) < 1.8));
    }
    assert.equal(item.border, "grass_border.png");
    const saved = JSON.parse(JSON.stringify(item));
    translate(saved, {x: 3, y: 2});
    for (const p of [{x: 7, y: 6}, {x: 9, y: 6}, {x: 11, y: 7}]) assert.ok(hit(saved, p, 0));
});

test("terrain joining follows touching regions transitively, leaving disconnected terrain separate", () => {
    const layer = newLayer("Ground");
    // Old projects may already contain several overlapping independent strokes.
    layer.items = [paint(10), paint(7), paint(4), paint(25)];
    addTerrainRegion(layer, paint(2), 2);
    assert.equal(layer.items.length, 2);
    assert.ok(hit(layer.items[0], {x: 25, y: 4}, 0));
    for (let x = 2; x <= 10; x += 0.25) assert.ok(hit(layer.items[1], {x, y: 4}, 0));
});

test("extending terrain retains holes and does not join a patch inside a hole", () => {
    const layer = newLayer("Ground");
    const trail = Array.from({length: 80}, (_, i) => ({
        x: 6 + 4 * Math.cos(i / 80 * Math.PI * 2),
        y: 6 + 4 * Math.sin(i / 80 * Math.PI * 2)
    }));
    const ring = paint(10, 6, 0.75);
    if (ring.kind.type !== "terrain_region") return;
    ring.kind.rings = terrainContours(trail, 0.75);
    layer.items.push(ring);
    addTerrainRegion(layer, paint(10.5, 6, 0.75), 0.75);
    assert.equal(layer.items.length, 1);
    assert.equal(hit(layer.items[0], {x: 6, y: 6}, 0), false);
    addTerrainRegion(layer, paint(6, 6, 0.75), 0.75);
    assert.equal(layer.items.length, 2);
    assert.equal(hit(layer.items[0], {x: 6, y: 6}, 0), false);
});

test("terrain matching respects material, border, tint, scale, and fill color", () => {
    const changes: ((item: Item) => void)[] = [
        i => {
            i.texture = "sand_tile.png";
        },
        i => {
            i.border = null;
        },
        i => {
            i.tint = [100, 255, 255, 255];
        },
        i => {
            i.texture_scale = 8;
        },
        i => {
            if (i.kind.type === "terrain_region") i.kind.color = [100, 255, 255, 255];
        },
    ];
    for (const change of changes) {
        const layer = newLayer("Ground");
        const first = paint(4), second = paint(5);
        change(first);
        layer.items.push(first);
        addTerrainRegion(layer, second, 2);
        assert.deepEqual(layer.items, [first, second]);
    }
});

test("terrain preserves paint order across other materials, architecture, water, and locked items", () => {
    const sand = paint(5);
    sand.texture = "sand_tile.png";
    const floor = newItem("Floor", {
        type: "floor",
        points: [{x: 3, y: 3}, {x: 7, y: 3}, {x: 7, y: 7}],
        color: [255, 255, 255, 255]
    });
    const water = paint(5);
    water.water = {
        deep_color: [0, 0, 255, 255],
        shallow_color: [0, 255, 255, 255],
        blend_distance: 1,
        disable_border: false
    };
    const locked = paint(5);
    locked.locked = true;
    for (const separator of [sand, floor, water, locked]) {
        const layer = newLayer("Ground");
        layer.items = [paint(4), separator];
        const before = structuredClone(layer.items);
        const fresh = paint(6);
        addTerrainRegion(layer, fresh, 2);
        assert.equal(layer.items.length, 3);
        assert.deepEqual(layer.items.slice(0, 2), before);
        assert.equal(layer.items[2], fresh);
    }
    for (const state of [{locked: true}, {visible: false}]) {
        const layer = {...newLayer("Ground"), ...state, items: [paint(4)]};
        const before = structuredClone(layer);
        assert.equal(addTerrainRegion(layer, paint(5), 2), undefined);
        assert.deepEqual(layer, before);
    }
});

test("terrain regions touching along an edge unite without joining separate patches", () => {
    const square = (x: number): Point[][] => [[{x, y: 0}, {x: x + 2, y: 0}, {x: x + 2, y: 2}, {x, y: 2}]];
    assert.equal(terrainRegionsTouch(square(0), square(2)), true);
    assert.equal(terrainRegionsTouch(square(0), square(2.25)), false);
    const layer = newLayer("Ground");
    for (const x of [0, 2]) {
        const item = paint(x);
        if (item.kind.type === "terrain_region") item.kind.rings = square(x);
        addTerrainRegion(layer, item, 1);
    }
    assert.equal(layer.items.length, 1);
    const item = layer.items[0];
    if (item.kind.type === "terrain_region") assert.equal(item.kind.rings.length, 1);
    assert.ok(hit(item, {x: 2, y: 1}, 0));
});
