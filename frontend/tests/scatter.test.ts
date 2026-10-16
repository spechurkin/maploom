import {test} from "node:test";
import assert from "node:assert/strict";
import {type AssetMeta, type Point} from "../src/map/model";
import {
    chooseScatterAssets,
    extendScatterStroke,
    scatterItem,
    type ScatterSettings,
    type ScatterStroke
} from "../src/tools/scatter";
import * as bridge from "../src/app/bridge";

const asset = (id: string, color_mode: AssetMeta["color_mode"] = "tint"): AssetMeta => ({
    id, name: id, path: `${id}.png`, tags: [], size: {x: 2, y: 3}, pivot: {x: 0.2, y: 0.7}, sha256: id, color_mode,
});
const settings: ScatterSettings = {
    rotation: [-110, 65], scale: [0.4, 1.7], spread: 0.5,
    randomColors: true, colors: [[90, 30, 20, 255], [30, 120, 60, 190]], color: [70, 80, 90, 255]
};

function rng(seed = 12345) {
    return () => {
        seed = (1664525 * seed + 1013904223) >>> 0;
        return seed / 2 ** 32;
    };
}

test("Scatter selects only the chosen pool, keeps aspect and pivot, and samples all transforms and palette colors", () => {
    const pool = [asset("a"), asset("b", "red_mask"), asset("c", "none")], random = rng();
    const seen = new Set<string>(), angles = new Set<number>(), scales = new Set<number>(), colors = new Set<string>();
    for (let n = 0; n < 300; n++) {
        const item = scatterItem(pool, {x: 3.1, y: 4.8}, settings, random)!;
        assert.equal(item.kind.type, "object");
        if (item.kind.type !== "object") continue;
        seen.add(item.kind.asset);
        angles.add(item.kind.rotation);
        scales.add(item.kind.size.x);
        assert.deepEqual(item.kind.position, {x: 3.1, y: 4.8});
        assert.deepEqual(item.kind.pivot, pool[0].pivot);
        assert.ok(Math.abs(item.kind.size.y / item.kind.size.x - 1.5) < 1e-10);
        assert.ok(item.kind.size.x >= 0.8 && item.kind.size.x <= 3.4);
        assert.ok(item.kind.rotation >= -110 * Math.PI / 180 && item.kind.rotation <= 65 * Math.PI / 180);
        if (item.kind.asset === "c") assert.deepEqual(item.tint, [255, 255, 255, 255]);
        else {
            assert.ok(settings.colors.some(color => JSON.stringify(color) === JSON.stringify(item.tint)));
            colors.add(String(item.tint));
        }
    }
    assert.equal(seen.size, 3);
    assert.equal(angles.size, 300);
    assert.equal(scales.size, 300);
    assert.equal(colors.size, 2);
});

test("empty pools, fixed ranges, palette fallback and disabled random colors", () => {
    assert.equal(scatterItem([], {x: 0, y: 0}, settings), null);
    for (const options of [{randomColors: false}, {colors: []}]) {
        const item = scatterItem([asset("fixed")], {x: 0, y: 0}, {
            ...settings, ...options,
            rotation: [35, 35],
            scale: [1.2, 1.2]
        })!;
        assert.deepEqual(item.tint, settings.color);
        assert.notEqual(item.tint, settings.color);
        if (item.kind.type === "object") {
            assert.equal(item.kind.size.x, 2.4);
            assert.equal(item.kind.rotation, 35 * Math.PI / 180);
        } else assert.fail("Expected object");
    }
});

test("Shift toggles objects without duplicates; plain clicks replace the set", () => {
    let selected = chooseScatterAssets([], "a", false);
    selected = chooseScatterAssets(selected, "b", true);
    selected = chooseScatterAssets(selected, "other-page", true);
    assert.deepEqual(selected, ["a", "b", "other-page"]);
    assert.deepEqual(chooseScatterAssets(selected, "b", true), ["a", "other-page"]);
    assert.deepEqual(chooseScatterAssets(selected, "new", false), ["new"]);
});

const path = (points: Point[], spread: number) => {
    const stroke: ScatterStroke = {last: points[0], remaining: spread};
    return [points[0], ...points.slice(1).flatMap(to => extendScatterStroke(stroke, to, spread))];
};
test("Spread is distance-based, carries remainder around corners, and is independent of pointer event frequency", () => {
    const coarse = path([{x: 0, y: 0}, {x: 4, y: 0}], 0.5);
    const fine = path(Array.from({length: 81}, (_, n) => ({x: n / 20, y: 0})), 0.5);
    assert.equal(coarse.length, 9);
    assert.equal(fine.length, 9);
    coarse.forEach((p, n) => assert.ok(Math.hypot(p.x - fine[n].x, p.y - fine[n].y) < 1e-10));
    assert.deepEqual(path([{x: 0, y: 0}, {x: 0.3, y: 0}, {x: 0.3, y: 0.7}], 0.5),
        [{x: 0, y: 0}, {x: 0.3, y: 0.2}, {x: 0.3, y: 0.7}]);
    assert.equal(path([{x: 0, y: 0}, {x: 4, y: 0}], 1).length, 5);
    const stroke = {last: {x: 0, y: 0}, remaining: 0.5};
    assert.deepEqual(extendScatterStroke(stroke, {x: 0, y: 0}, 0.5), []);
    assert.deepEqual(extendScatterStroke(stroke, {x: 4, y: 0}, 0), []);
});

test("a complete stroke survives serialization and commits as one undoable/redoable operation in the preview bridge", async () => {
    const before = await bridge.snapshot(), project = structuredClone(before.project);
    const random = rng();
    const items = path([{x: 2, y: 2}, {
        x: 5,
        y: 2
    }], 0.5).map(point => scatterItem([asset("a")], point, settings, random)!);
    project.levels[0].layers[1].items.push(...JSON.parse(JSON.stringify(items)));
    const placed = await bridge.act({type: "commit", project}, before.revision);
    assert.deepEqual(placed.project.levels[0].layers[1].items, items);
    const undone = await bridge.act({type: "undo"}, placed.revision);
    assert.equal(undone.project.levels[0].layers[1].items.length, 0);
    const redone = await bridge.act({type: "redo"}, undone.revision);
    assert.deepEqual(redone.project.levels[0].layers[1].items, items);
});
