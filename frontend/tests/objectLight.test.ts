import {test} from "node:test";
import assert from "node:assert/strict";
import {act, snapshot} from "../src/app/bridge";
import {newItem} from "../src/map/model";

test("object light blocking survives editor commits, undo, redo and duplication", async () => {
    let state = await snapshot();
    const project = structuredClone(state.project);
    const item = newItem("Crate", {
        type: "object", asset: "crate", position: {x: 6, y: 5}, size: {x: 1, y: 2},
        rotation: 0, pivot: {x: 0.5, y: 0.5}, flip_x: false, flip_y: false,
    });
    project.levels[0].layers[2].items = [item];
    state = await act({type: "commit", project}, state.revision);
    const enabled = structuredClone(state.project);
    enabled.levels[0].layers[2].items[0].blocks_light = true;
    state = await act({type: "commit", project: enabled}, state.revision);
    assert.equal(state.project.levels[0].layers[2].items[0].blocks_light, true);
    state = await act({type: "undo"}, state.revision);
    assert.equal(state.project.levels[0].layers[2].items[0].blocks_light ?? false, false);
    state = await act({type: "redo"}, state.revision);
    assert.equal(state.project.levels[0].layers[2].items[0].blocks_light, true);
    state = await act({type: "duplicate", level: 0, ids: [item.id]}, state.revision);
    const copy = state.project.levels[0].layers[2].items[1];
    assert.notEqual(copy.id, item.id);
    assert.equal(copy.blocks_light, true);
});
