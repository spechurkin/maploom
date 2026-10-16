import {test} from "node:test";
import assert from "node:assert/strict";
import {newItem, newLevel, type Project} from "../src/map/model";
import {addLevel, mainLevelIndex, moveLevel, protectMainLevel, removeLevel} from "../src/map/levels";
import {previewGpuScene} from "../src/rendering/gpu";

const project = (): Project => ({
    format: "maploom/project", version: 1, id: "test", name: "test",
    width: 8, height: 8, pixels_per_grid: 32, background: [120, 150, 180, 255],
    background_asset: null, use_default_assets: false, levels: [newLevel("Main")],
    assets: {}, packs: [], asset_catalogs: [], external_assets: {}, extensions: {}
});

test("commit validation rejects main-floor removal, replacement and crossing", () => {
    const p = project();
    addLevel(p, "Above", 0);
    const moved = structuredClone(p);
    moved.levels.reverse();
    assert.throws(() => protectMainLevel(p, moved), /Main floor is fixed/);
    const removed = structuredClone(p);
    removed.levels.shift();
    assert.throws(() => protectMainLevel(p, removed), /Main floor is fixed/);
    const replaced = structuredClone(p);
    replaced.main_level = replaced.levels[1].id;
    assert.throws(() => protectMainLevel(p, replaced), /Main floor is fixed/);
    const basement = structuredClone(p);
    addLevel(basement, "Basement", 0, true);
    assert.doesNotThrow(() => protectMainLevel(p, basement));
});

test("legacy main floor stays pinned when inserting, ordering, deleting and saving floors", () => {
    const p = project(), main = p.levels[0].id;
    const above = addLevel(p, "Above", 0)!;
    const top = addLevel(p, "Top", 1)!;
    const below = addLevel(p, "Below", 0, true)!;
    const bottom = addLevel(p, "Bottom", 0, true)!;
    assert.equal(p.main_level, main);
    assert.equal(mainLevelIndex(p), 2);
    assert.equal(moveLevel(p, main, 1), false);
    assert.equal(moveLevel(p, main, -1), false);
    assert.equal(moveLevel(p, above, -1), false);
    assert.equal(moveLevel(p, below, 1), false);
    assert.equal(moveLevel(p, above, 1), true);
    assert.equal(moveLevel(p, below, -1), true);
    assert.deepEqual(p.levels.map(l => l.id), [below, bottom, main, top, above]);
    assert.equal(removeLevel(p, main), false);
    assert.equal(removeLevel(p, above), true);
    const saved = JSON.parse(JSON.stringify(p));
    assert.equal(saved.levels[mainLevelIndex(saved)].id, main);
    assert.deepEqual(saved.levels.map((l: { id: string }) => l.id), [below, bottom, main, top]);
});

test("floor deletion respects locked content and floor count is limited", () => {
    const p = project(), id = addLevel(p, "Above", 0)!;
    p.levels[1].layers[0].locked = true;
    assert.equal(removeLevel(p, id), false);
    p.levels[1].layers[0].locked = false;
    const item = newItem("text", {
        type: "text",
        position: {x: 1, y: 1},
        text: "text",
        size: 1,
        color: [255, 255, 255, 255]
    });
    item.locked = true;
    p.levels[1].layers[0].items.push(item);
    assert.equal(removeLevel(p, id), false);
    assert.equal(moveLevel(p, id, 1), false);
    while (p.levels.length < 256) addLevel(p, "Above", p.levels.length - 1);
    assert.equal(addLevel(p, "Too many", 0), null);
});

test("preview puts all lower surfaces in one underlay and keeps basements isolated", () => {
    const p = project();
    addLevel(p, "Above", 0);
    addLevel(p, "Top", 1);
    addLevel(p, "Below", 0, true);
    const scenes = p.levels.map((_, i) => previewGpuScene(p, i, 0, false));
    assert.equal(scenes[0].underlay_meshes, 0);
    assert.equal(scenes[1].underlay_meshes, 0);
    assert.equal(scenes[2].underlay_meshes, scenes[0].meshes.length + scenes[1].meshes.length);
    assert.equal(scenes[3].underlay_meshes, scenes[2].meshes.length);
    // Decode without relying on Node Buffer's pooled backing buffer offset.
    const colors = (i: number) => Array.from(new Float32Array(Uint8Array.from(Buffer.from(scenes[i].meshes.at(-1)!.vertices, "base64")).buffer).slice(4, 8));
    assert.deepEqual(colors(0), [0, 0, 0, 1]);
    assert.equal(colors(1)[3], 1);
    assert.deepEqual(colors(2), [0, 0, 0, 0]);
});
