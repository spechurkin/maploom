import {test} from "node:test";
import assert from "node:assert/strict";
import {builtInMapSizes, resizeMap, validMapSize, zeroEdges} from "../src/map/mapSize";
import {act, snapshot} from "../src/app/bridge";
import {newItem, newLevel, type Project} from "../src/map/model";

test("size limits reject empty, fractional and non-finite dimensions", () => {
    for (const n of [0, -1, 513, 2.5, NaN, Infinity]) assert.equal(validMapSize(n, 32), false);
    assert.ok(validMapSize(1, 512));
    for (const preset of builtInMapSizes) assert.ok(validMapSize(preset.width, preset.height));
});

test("resize shifts hidden and locked content on all levels and retains off-map content", async () => {
    const project: Project = structuredClone((await snapshot()).project);
    project.levels = [newLevel("Ground"), newLevel("Upstairs")];
    const floor = newItem("Floor", {
        type: "floor",
        points: [{x: 1, y: 1}, {x: 5, y: 1}, {x: 5, y: 5}],
        color: [255, 255, 255, 255]
    });
    floor.floor_holes = [[{x: 2, y: 2}, {x: 3, y: 2}, {x: 3, y: 3}]];
    floor.water_source_rings = [[{x: 1, y: 1}, {x: 5, y: 1}, {x: 5, y: 5}]];
    floor.locked = true;
    project.levels[0].layers[0].locked = true;
    project.levels[0].layers[0].visible = false;
    project.levels[0].layers[0].items = [floor];
    project.levels[1].layers[0].items = [newItem("Text", {
        type: "text",
        position: {x: 2, y: 3},
        text: "Up",
        size: 1,
        color: [255, 255, 255, 255]
    })];
    const next = resizeMap(project, {left: -3, top: 2, right: 4, bottom: -1});
    assert.equal(next.width, project.width + 1);
    assert.equal(next.height, project.height + 1);
    const shifted = next.levels[0].layers[0].items[0];
    assert.deepEqual(shifted.floor_holes?.[0][0], {x: -1, y: 4});
    assert.deepEqual(shifted.water_source_rings?.[0][0], {x: -2, y: 3});
    assert.deepEqual(next.levels[1].layers[0].items[0].kind, {
        ...project.levels[1].layers[0].items[0].kind,
        position: {x: -1, y: 5}
    });
    assert.deepEqual(resizeMap(next, {left: 3, top: -2, right: -4, bottom: 1}), project);
    assert.throws(() => resizeMap(project, {...zeroEdges(), left: -512}));
    assert.throws(() => resizeMap(project, {...zeroEdges(), top: 0.5}));
    assert.deepEqual(project.levels[0].layers[0].items[0], floor);
});

test("preview starts without an open project; new and resize support undo/redo", async () => {
    let s = await snapshot();
    assert.equal(s.project_open, false);
    s = await act({type: "new", name: "Test", width: 35, height: 20, demo: false}, s.revision);
    assert.equal(s.project_open, true);
    assert.equal(s.project.width, 35);
    assert.ok(s.project.levels.every(l => l.layers.every(l => l.items.length === 0)));
    assert.equal(s.dirty, false);
    s = await act({type: "resize_map", top: 2, right: 0, bottom: -1, left: 3}, s.revision);
    assert.deepEqual([s.project.width, s.project.height], [38, 21]);
    assert.equal(s.dirty, true);
    s = await act({type: "undo"}, s.revision);
    assert.deepEqual([s.project.width, s.project.height], [35, 20]);
    assert.equal(s.dirty, false);
    s = await act({type: "redo"}, s.revision);
    assert.deepEqual([s.project.width, s.project.height], [38, 21]);
});
