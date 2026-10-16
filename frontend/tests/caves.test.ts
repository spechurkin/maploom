import {test} from "node:test";
import assert from "node:assert/strict";
import {type Asset, type CaveStyle, hit, newItem, newLevel, paintOrder, pick, translate} from "../src/map/model";
import {caveFootprint, caveStyles, outsideCaveMasks, paintCave} from "../src/tools/caves";
import {appendTerrainPoint, terrainContours} from "../src/tools/terrain";

const color: CaveStyle = {
    colorable: true,
    ground_color: [127, 126, 113, 255],
    wall_color: [127, 126, 113, 255],
    blasts: []
};
const asset = (style: string, part: string): Asset => ({
    category: "caves",
    builtin: true,
    meta: {
        id: `res://textures/caves/${style}/${part}.png`,
        name: part,
        path: "",
        tags: [style],
        size: {x: 8, y: part === "wall" ? 1 : 8},
        pivot: {x: 0.5, y: 0.5},
        sha256: "",
        color_mode: style === "colorable" ? "tint" : "none"
    }
});
const materials = caveStyles([asset("colorable", "floor"), asset("colorable", "wall"), asset("rocky", "floor"), asset("rocky", "wall")]);
test("cave style pairs resolve separate ground and rock textures and colorable capability", () => {
    assert.equal(materials.length, 2);
    assert.equal(materials[0].colorable, true);
    assert.equal(materials[1].colorable, false);
    assert.equal(caveStyles([asset("colorable", "floor")]).length, 0);
});
test("dig unites connected strokes across layers and blast cuts ground with a persistent scenery mask", () => {
    const level = newLevel("Map"), target = level.layers[1];
    paintCave(level, target, [{x: 5, y: 5}], 3, "dig", color, materials[0], "Cave");
    paintCave(level, level.layers[2], [{x: 9, y: 5}], 3, "dig", color, materials[1], "Cave");
    const items = level.layers.flatMap(l => l.items);
    assert.equal(items.length, 1);
    const c = items[0];
    assert.equal(c.cave?.colorable, false);
    assert.ok(hit(c, {x: 7, y: 5}, 0));
    const trail = [{x: 7, y: 1}];
    appendTerrainPoint(trail, {x: 7, y: 8}, 1);
    paintCave(level, target, trail, 1, "blast", color, undefined, "Cave");
    assert.ok(!hit(c, {x: 7, y: 5}, 0));
    assert.ok(hit(c, {x: 4, y: 5}, 0));
    assert.ok(c.cave!.blasts.length);
    assert.equal(c.wall_attachment, null);
    const saved = JSON.parse(JSON.stringify(c));
    translate(saved, {x: 2, y: 3});
    assert.ok(!hit(saved, {x: 9, y: 8}, 0));
    assert.ok(hit(saved, {x: 6, y: 8}, 0));
});
test("dig fills a blast again without closing unrelated holes or changing disconnected caves", () => {
    const level = newLevel("Map"), l = level.layers[1];
    paintCave(level, l, [{x: 5, y: 5}], 3, "dig", color, materials[0], "Cave");
    paintCave(level, l, [{x: 20, y: 5}], 2, "dig", color, materials[1], "Cave");
    const separate = structuredClone(l.items[1]);
    paintCave(level, l, [{x: 5, y: 5}], 1, "blast", color, undefined, "Cave");
    assert.ok(!hit(l.items[0], {x: 5, y: 5}, 0));
    paintCave(level, l, [{x: 5, y: 5}], 0.5, "dig", color, materials[0], "Cave");
    assert.ok(hit(l.items[0], {x: 5, y: 5}, 0));
    assert.ok(!hit(l.items[0], {x: 5.8, y: 5}, 0));
    assert.deepEqual(l.items[1], separate);
});
test("locked cave bodies and layers reject a stroke atomically, hidden caves stay separate", () => {
    const level = newLevel("Map"), l = level.layers[1];
    paintCave(level, l, [{x: 5, y: 5}], 3, "dig", color, materials[0], "Cave");
    l.items[0].locked = true;
    const before = structuredClone(level);
    assert.equal(paintCave(level, l, [{x: 5, y: 5}], 1, "blast", color, undefined, "Cave"), null);
    assert.deepEqual(level, before);
    assert.equal(paintCave(level, l, [{x: 8, y: 5}], 2, "dig", color, materials[0], "Cave"), null);
    assert.deepEqual(level, before);
    l.visible = false;
    paintCave(level, level.layers[2], [{x: 5, y: 5}], 2, "dig", color, materials[0], "Cave");
    assert.equal(level.layers[2].items.length, 1);
});
test("room floors win picking in either order and crossing cuts keep only external room walls", () => {
    for (const first of [false, true]) {
        const level = newLevel("Map"), l = level.layers[1];
        const room = newItem("Room", {
            type: "room",
            rings: [[{x: 4, y: 2}, {x: 8, y: 2}, {x: 8, y: 6}, {x: 4, y: 6}]],
            floor: "wood",
            wall: "stone",
            width: 0.2
        });
        if (first) l.items.push(room);
        paintCave(level, l, [{x: 6, y: 6}], 4, "dig", color, materials[0], "Cave");
        if (!first) l.items.push(room);
        assert.equal(pick(level, {x: 6, y: 4}, 0)?.id, room.id);
        assert.equal(paintOrder(level).at(-1), room);
        const mask = caveFootprint(l.items.find(i => i.cave)!);
        assert.equal(outsideCaveMasks({x: 4, y: 6}, {x: 8, y: 6}, mask).length, 0);
        const rings = terrainContours([], 1, {
            polygon: room.kind.type === "room" ? room.kind.rings[0] : [],
            smoothing: 0
        });
        assert.equal(outsideCaveMasks({x: 4, y: 2}, {x: 8, y: 2}, rings, false).length, 1);
        assert.equal(outsideCaveMasks({x: 4, y: 2}, {x: 8, y: 2}, rings, true).length, 0);
    }
});
