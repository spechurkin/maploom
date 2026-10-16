import test from "node:test";
import assert from "node:assert/strict";
import {type Level, newItem, pick, prefabParts, setObjectSize, translate} from "../src/map/model";
import {canCreatePrefab, capturePrefab, placePrefab, prefabBounds, prefabInstance} from "../src/map/prefabs";
import {act} from "../src/app/bridge";
import {snapObjectsToWalls} from "../src/tools/wallSnap";

const object = (x: number, y: number) => newItem("Chair", {
    type: "object", asset: "chair", position: {x, y}, size: {x: 2, y: 1},
    pivot: {x: 0.25, y: 0.5}, rotation: 0, flip_x: true, flip_y: false,
});

test("clicking any prefab part selects its sole parent and leaves gaps unselected", () => {
    const path = newItem("Rope", {
        type: "path",
        points: [{x: -2, y: 3}, {x: 2, y: 3}],
        width: 0.2,
        color: [255, 255, 255, 255]
    });
    const fill = newItem("Fill", {
        type: "floor",
        points: [{x: -1, y: -4}, {x: 1, y: -4}, {x: 0, y: -2}],
        color: [255, 255, 255, 255]
    });
    const item = prefabInstance({
        id: "mixed",
        name: "Combined",
        items: [object(-4, 0), object(4, 0), path, fill]
    }, {x: 10, y: 10});
    const level: Level = {
        id: "level",
        name: "Level",
        layers: [{id: "layer", name: "Layer", visible: true, locked: false, opacity: 1, items: [item]}]
    };
    for (const p of [{x: 6, y: 10}, {x: 14, y: 10}, {x: 10, y: 13}, {x: 10, y: 7}])
        assert.equal(pick(level, p, 0.01)?.id, item.id);
    assert.equal(pick(level, {x: 10, y: 10}, 0.01), undefined);
    assert.equal(level.layers[0].items.length, 1);
});

test("one prefab transform moves, rotates, scales and mirrors all parts with no source mutation", () => {
    const definition = capturePrefab("Pair", [object(-4, 0), object(4, 0)]);
    const item = prefabInstance(definition, {x: 10, y: 10});
    const original = structuredClone(item.prefab);
    translate(item, {x: 2, y: 3});
    assert.ok(item.kind.type === "object");
    setObjectSize(item, "x", item.kind.size.x * 2);
    assert.equal(item.kind.size.y, original!.size.y * 2);
    item.kind.rotation = Math.PI / 2;
    const parts = prefabParts(item);
    assert.ok(parts[0].kind.type === "object" && parts[1].kind.type === "object");
    assert.ok(Math.abs(parts[1].kind.position.y - parts[0].kind.position.y - 16) < 1e-9);
    item.kind.flip_x = true;
    const mirrored = prefabParts(item);
    assert.ok(mirrored[0].kind.type === "object" && mirrored[1].kind.type === "object");
    assert.ok(Math.abs(mirrored[1].kind.position.y - mirrored[0].kind.position.y + 16) < 1e-9);
    assert.deepEqual(item.prefab, original);
    const captured = capturePrefab("Nested pair", [item, object(1, 1)]);
    assert.equal(captured.items.length, 3);
    assert.ok(captured.items.every(i => !i.prefab));
});

test("editor placement, move, duplicate, undo and deletion address a single prefab record", async () => {
    let state = await act({type: "new", name: "Prefabs", width: 32, height: 32, demo: false}, 0);
    const a = object(2, 2), b = object(6, 2);
    for (const item of [a, b]) state = await act({type: "add", level: 0, layer: 1, item}, state.revision);
    state = await act({type: "save_prefab", level: 0, ids: [a.id, b.id], name: "Pair"}, state.revision);
    state = await act({
        type: "place_prefab", key: state.prefabs!.at(-1)!.key, level: 0, layer: 1,
        position: {x: 12, y: 12}, scale: 1, rotation: 0
    }, state.revision);
    const root = state.project.levels[0].layers[1].items.at(-1)!;
    assert.equal(state.project.levels[0].layers[1].items.length, 3);
    assert.equal(root.prefab?.items.length, 2);
    state = await act({type: "move", level: 0, ids: [root.id], delta: {x: 1, y: 2}}, state.revision);
    state = await act({type: "duplicate", level: 0, ids: [root.id]}, state.revision);
    assert.equal(state.project.levels[0].layers[1].items.length, 4);
    state = await act({type: "undo"}, state.revision);
    assert.equal(state.project.levels[0].layers[1].items.length, 3);
    state = await act({type: "delete", level: 0, ids: [root.id]}, state.revision);
    assert.deepEqual(state.project.levels[0].layers[1].items.map(i => i.id), [a.id, b.id]);
});

test("wall snapping uses the combined geometry rather than the thumbnail's dimensions", () => {
    const prefab = capturePrefab("Pair", [object(-4, 0), object(4, 0)]);
    const item = prefabInstance(prefab, {x: 10, y: 10});
    const shift = snapObjectsToWalls([item], {x: 0, y: 0}, [{
        a: {x: 4.8, y: 5},
        b: {x: 4.8, y: 15},
        width: 0.2
    }], new Map(), 100, 0.3);
    assert.ok(shift);
    assert.ok(Math.abs(shift.x + 0.05) < 1e-9);
    assert.equal(shift.y, 0);
});

test("mixed imported compositions transform their paths, fills, and sampled profiles together", () => {
    const path = newItem("Rope", {
        type: "path",
        points: [{x: -1, y: 0}, {x: 1, y: 0}],
        width: 0.05,
        color: [255, 255, 255, 255]
    });
    path.texture_scale = 0.2;
    path.path_profile = {widths: [0, 1], alpha: [1, 0]};
    const fill = newItem("Fill", {
        type: "floor",
        points: [{x: 0, y: 0}, {x: 1, y: 0}, {x: 0, y: 1}],
        color: [255, 255, 255, 255]
    });
    fill.floor_outline = false;
    const prefab = {id: "dd:sample", name: "DD", items: [object(0, 0), path, fill], preview: "preview:image"};
    assert.equal(canCreatePrefab(prefab.items), false);
    const placed = placePrefab(prefab, {x: 10, y: 20}, 2, Math.PI / 2);
    const rope = placed[1];
    assert.ok(rope.kind.type === "path");
    assert.deepEqual(rope.kind.points, [{x: 10, y: 18}, {x: 10, y: 22}]);
    assert.equal(rope.kind.width, 0.1);
    assert.equal(rope.texture_scale, 0.4);
    assert.deepEqual(rope.path_profile, path.path_profile);
    assert.equal(placed[2].floor_outline, false);
    assert.deepEqual(prefab.items[1], path);
});

test("prefabs require multiple objects and preserve the source selection", () => {
    const a = object(10, 5), b = object(14, 5);
    a.locked = true;
    a.tint = [30, 70, 90, 150];
    a.wall_attachment = {wall: "old-wall", segment: 0, t: 0.5, length: 1};
    assert.equal(canCreatePrefab([a]), false);
    assert.equal(canCreatePrefab([a, newItem("Text", {
        type: "text",
        text: "x",
        position: {x: 0, y: 0},
        size: 1,
        color: [255, 255, 255, 255]
    })]), false);
    const original = structuredClone([a, b]);
    const prefab = capturePrefab("  Pair  ", [a, b]);
    assert.equal(prefab.name, "Pair");
    assert.deepEqual([a, b], original);
    assert.equal(prefab.items[0].wall_attachment, null);
    assert.equal(prefab.items[0].locked, false);
    const bounds = prefabBounds(prefab.items);
    assert.equal(bounds.x + bounds.right, 0);
    assert.equal(bounds.y + bounds.bottom, 0);
});

test("placement rotates and scales the entire composition, retaining per-object styling", () => {
    const a = object(0, 0), b = object(4, 0);
    a.tint = [30, 70, 90, 150];
    a.blocks_light = true;
    const prefab = capturePrefab("Pair", [a, b]);
    const placed = placePrefab(prefab, {x: 20, y: 20}, 2, Math.PI / 2);
    const first = placed[0].kind, second = placed[1].kind;
    assert.ok(first.type === "object" && second.type === "object");
    assert.ok(Math.abs(first.position.x - second.position.x) < 1e-10);
    assert.ok(Math.abs(second.position.y - first.position.y - 8) < 1e-10);
    assert.deepEqual(first.size, {x: 4, y: 2});
    assert.deepEqual(first.pivot, {x: 0.25, y: 0.5});
    assert.equal(first.flip_x, true);
    assert.equal(first.rotation, Math.PI / 2);
    assert.deepEqual(placed[0].tint, a.tint);
    assert.equal(placed[0].blocks_light, true);
    assert.notEqual(placed[0].id, prefab.items[0].id);
    assert.notEqual(placed[0].id, placePrefab(prefab, {x: 20, y: 20})[0].id);
    assert.equal(placePrefab(prefab, {x: 0, y: 0}, 1, 0, false)[0].id, prefab.items[0].id);
});
