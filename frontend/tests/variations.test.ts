import {test} from "node:test";
import assert from "node:assert/strict";
import {addVariation, selectVariation, removeVariation, variationOptions} from "../src/map/variations";
import {resizeMap} from "../src/map/mapSize";
import {act, snapshot} from "../src/app/bridge";
import {newLevel, newItem, type Project} from "../src/map/model";

const fixture = (): Project => ({
    format: "maploom/project", version: 1, id: "test", name: "test",
    width: 8, height: 8, pixels_per_grid: 32, background: [80, 90, 100, 255], background_asset: null,
    use_default_assets: false, levels: [newLevel("Main"), newLevel("Tower")], assets: {}, packs: [],
    asset_catalogs: [], external_assets: {}, extensions: {}
});

test("adding copies the selected variation's complete scene, and floors remain independent", () => {
    const p = fixture();
    p.levels[0].layers[0].locked = true;
    p.levels[1].layers[1].items.push(newItem("Label", {
        type: "text", text: "Tower",
        position: {x: 2, y: 3}, size: 1, color: [255, 255, 255, 255]
    }));
    const original = structuredClone(p);
    assert.equal(variationOptions(p).length, 1);
    assert.throws(() => removeVariation(p), /last variation/);
    addVariation(p, "Night");
    assert.deepEqual(p.levels, original.levels);
    const night = p.active_variation!;
    p.levels.pop();
    p.background = [1, 2, 3, 255];
    selectVariation(p, p.id);
    assert.deepEqual(p.levels, original.levels);
    assert.deepEqual(p.background, original.background);
    addVariation(p, "Day copy");
    assert.deepEqual(p.levels, original.levels);
    selectVariation(p, night);
    assert.equal(p.levels.length, 1);
    assert.deepEqual(p.background, [1, 2, 3, 255]);
    removeVariation(p);
    assert.equal(p.variations!.length, 2);
    assert.ok(p.variations!.some(v => v.id === p.active_variation));
});

test("resize shifts inactive and active copies once on a common canvas", () => {
    const p = fixture();
    p.levels[0].layers[0].items.push(newItem("Label", {
        type: "text", text: "Label",
        position: {x: 2, y: 3}, size: 1, color: [255, 255, 255, 255]
    }));
    addVariation(p, "Night");
    const next = resizeMap(p, {top: 2, left: 3, right: 0, bottom: 0});
    for (const v of next.variations!) {
        const kind = v.levels[0].layers[0].items[0].kind;
        assert.equal(kind.type, "text");
        if (kind.type === "text") assert.deepEqual(kind.position, {x: 5, y: 5});
    }
});

test("bridge actions preserve edits across switching, deletion and undo/redo", async () => {
    let s = await snapshot();
    s = await act({type: "new", name: "Test", width: 8, height: 8, demo: false}, s.revision);
    const base = s.project.id;
    s = await act({type: "add_variation", name: "Night"}, s.revision);
    const night = s.project.active_variation;
    const changed = structuredClone(s.project);
    changed.levels.push(newLevel("Rooftop"));
    s = await act({type: "commit", project: changed}, s.revision);
    s = await act({type: "select_variation", id: base}, s.revision);
    assert.equal(s.project.levels.length, 1);
    s = await act({type: "select_variation", id: night}, s.revision);
    assert.equal(s.project.levels.length, 2);
    s = await act({type: "rename_variation", name: "Dusk"}, s.revision);
    assert.equal(s.project.variations![1].name, "Dusk");
    const before = structuredClone(s.project);
    s = await act({type: "remove_variation"}, s.revision);
    assert.equal(s.project.levels.length, 1);
    s = await act({type: "undo"}, s.revision);
    assert.deepEqual(s.project, before);
    s = await act({type: "redo"}, s.revision);
    assert.equal(s.project.variations!.length, 1);
    await assert.rejects(act({type: "remove_variation"}, s.revision), /last variation/);
    const invalid = structuredClone(s.project);
    invalid.active_variation = "missing";
    await assert.rejects(act({type: "commit", project: invalid}, s.revision), /variation actions/);
});
