import {test} from "node:test";
import assert from "node:assert/strict";
import {
    defaultPathStyle,
    itemPathRibbon,
    pathPoints,
    pathRibbon,
    setPathTexture,
    simplifyPath
} from "../src/tools/paths";
import {type AssetMeta, bounds, hit, newItem, newLevel, pickPath} from "../src/map/model";
import {act, snapshot} from "../src/app/bridge";
import {
    geometryRings,
    insertVertex,
    moveVertex,
    removeVertex,
    supportsGeometry,
    validGeometry
} from "../src/tools/architecture";

test("sampled DD profiles preserve original widths and opacity without resmoothing", () => {
    const item = newItem("DD rope", {
        type: "path",
        points: [{x: 0, y: 0}, {x: 1, y: 0}, {x: 2, y: 0}],
        width: 0.05,
        color: [255, 255, 255, 255]
    });
    item.path_profile = {widths: [0, 0.5, 1], alpha: [1, 0.5, 0]};
    const vertices = itemPathRibbon(item);
    assert.equal(vertices.length, 6);
    assert.equal(vertices[0].position.y, 0);
    assert.equal(vertices[2].position.y, 0.0125);
    assert.equal(vertices[4].position.y, 0.025);
    assert.deepEqual(vertices.map(v => v.alpha), [1, 1, 0.5, 0.5, 0, 0]);
});

const ps = [{x: 2, y: 4}, {x: 8, y: 4}, {x: 8, y: 10}, {x: 14, y: 10}];
const item = () => newItem("Path", {
    type: "path",
    points: structuredClone(ps),
    width: 0.5,
    color: [255, 255, 255, 255]
}, "rail");
test("smoothness changes the bend geometry, preserves control points and leaves alpha and UVs sharp", () => {
    const sharp = pathPoints(ps, 0), smooth = pathPoints(ps, 1);
    assert.deepEqual(smooth[0], ps[0]);
    assert.deepEqual(smooth.at(-1), ps.at(-1));
    assert.ok(ps.every(p => smooth.some(q => Math.hypot(q.x - p.x, q.y - p.y) < 1e-8)));
    assert.ok(sharp.every(p => p.y === 4 || p.x === 8 || p.y === 10));
    assert.ok(smooth.some(p => p.y < 3.8));
    const mesh = pathRibbon(ps, 0.5, 2, {...defaultPathStyle(), smoothness: 1});
    assert.ok(mesh.every(v => v.alpha === 1 && (v.uv.y === 0 || v.uv.y === 1)));
    assert.ok(mesh.filter((_, n) => n % 2 === 0).every((v, n, a) => n === 0 || v.uv.x >= a[n - 1].uv.x));
});
test("fade changes opacity; shrink changes the width; UVs remain continuous through both", () => {
    const mesh = pathRibbon([{x: 0, y: 0}, {x: 10, y: 0}], 1, 2, {
        ...defaultPathStyle(),
        transition_in: "fade",
        transition_out: "shrink"
    });
    assert.equal(mesh[0].alpha, 0);
    assert.equal(mesh[0].position.y, 0.5);
    assert.equal(mesh.at(-1)!.alpha, 1);
    assert.equal(mesh.at(-1)!.position.y, 0);
    assert.equal(mesh.at(-1)!.uv.x, 5);
    assert.ok(mesh.some(v => v.alpha > 0 && v.alpha < 1));
    const long = pathRibbon([{x: 0, y: 0}, {x: 512, y: 0}], 0.1, 2, {...defaultPathStyle(), transition_in: "fade"});
    assert.ok(long.some(v => Math.abs(v.position.x - 0.05) < 1e-7 && Math.abs(v.alpha - 0.5) < 1e-7));
    assert.ok(long.filter(v => v.position.x >= 0.1).every(v => v.alpha === 1));
});
test("picking and bounds follow the curved ribbon; editing uses the original control points", () => {
    const i = item();
    i.path_style = {...defaultPathStyle(), smoothness: 1};
    assert.ok(supportsGeometry(i));
    assert.equal(geometryRings(i)[0].length, 4);
    const ribbon = itemPathRibbon(i), n = 20;
    const center = {
        x: (ribbon[n].position.x + ribbon[n + 1].position.x) / 2,
        y: (ribbon[n].position.y + ribbon[n + 1].position.y) / 2
    };
    assert.ok(hit(i, center, 0));
    assert.ok(bounds(i).y < 3.8);
    assert.ok(!hit(i, {x: 20, y: 20}, 0));
    insertVertex(i, {ring: 0, point: 1}, {x: 5, y: 4});
    assert.equal(geometryRings(i)[0].length, 5);
    moveVertex(i, {ring: 0, point: 1}, {x: 5, y: 3});
    assert.ok(validGeometry(i));
    assert.ok(removeVertex(i, {ring: 0, point: 1}));
    assert.deepEqual(geometryRings(i)[0], ps);
});
test("width and smoothness accept their boundaries and reject values outside them", () => {
    const i = item();
    for (const width of [0.00390625, 0.099, 0.1, 5, 5.001, 512]) {
        if (i.kind.type === "path") i.kind.width = width;
        assert.ok(validGeometry(i));
    }
    for (const width of [0, -0.1, 512.001, NaN]) {
        if (i.kind.type === "path") i.kind.width = width;
        assert.ok(!validGeometry(i));
    }
    if (i.kind.type === "path") i.kind.width = 0.5;
    for (const smoothness of [0, 1]) {
        i.path_style = {...defaultPathStyle(), smoothness};
        assert.ok(validGeometry(i));
    }
    for (const smoothness of [-0.01, 1.01, NaN]) {
        i.path_style = {...defaultPathStyle(), smoothness};
        assert.ok(!validGeometry(i));
    }
});
test("freehand simplification removes collinear samples while preserving turns and stroke endpoints", () => {
    assert.deepEqual(simplifyPath([{x: 0, y: 0}, {x: 1, y: 0.001}, {x: 2, y: 0}, {x: 2, y: 2}], 0.01), [{
        x: 0,
        y: 0
    }, {x: 2, y: 0}, {x: 2, y: 2}]);
    const closed = [{x: 0, y: 0}, {x: 2, y: 0}, {x: 2, y: 2}, {x: 0, y: 0}];
    assert.deepEqual(simplifyPath(closed, 0.01), closed);
});

test("path editing picks the top visible path across layers through other item types", () => {
    const level = newLevel("Paths"), lower = item(), upper = item();
    const floor = newItem("Floor", {
        type: "floor",
        points: [{x: 0, y: 0}, {x: 20, y: 0}, {x: 20, y: 20}, {x: 0, y: 20}],
        color: [255, 255, 255, 255]
    });
    level.layers[1].items.push(lower);
    level.layers[2].items.push(upper, floor);
    assert.equal(pickPath(level, ps[0], 0)?.id, upper.id);
    level.layers[2].visible = false;
    assert.equal(pickPath(level, ps[0], 0)?.id, lower.id);
    assert.equal(pickPath(level, {x: 19, y: 19}, 0), undefined);
    level.layers[1].locked = true;
    lower.locked = true;
    // A locked path can be inspected; the editor disables mutations.
    assert.equal(pickPath(level, ps[0], 0)?.id, lower.id);
});

test("changing path texture preserves repeat multiplier, geometry and style", () => {
    const texture = (id: string, x: number, y: number): AssetMeta => ({
        id, name: id, path: id, size: {x, y}, tags: [], pivot: {x: 0.5, y: 0.5}, sha256: "", color_mode: "tint"
    });
    const old = texture("rail", 4, 1), next = texture("road", 3, 2), i = item();
    i.texture_scale = 6; // 4 / 1 * 0.5 cells wide * 3x repeat.
    i.path_style = {...defaultPathStyle(), smoothness: 0.7, transition_in: "fade", blocks_light: true};
    const original = structuredClone(i);
    setPathTexture(i, next, old);
    assert.equal(i.texture, next.id);
    assert.equal(i.texture_scale, 2.25);
    assert.deepEqual({...i, texture: original.texture, texture_scale: original.texture_scale}, original);
    setPathTexture(i, old, next);
    assert.deepEqual(i, original);
    setPathTexture(i, next);
    assert.equal(i.texture_scale, 0.75);
    const narrow = texture("narrow", 0.01, 2);
    setPathTexture(i, narrow, next);
    const narrowScale = i.texture_scale;
    setPathTexture(i, narrow, narrow);
    assert.equal(i.texture_scale, narrowScale);
});

test("placed path point and texture edits survive undo, redo and JSON round trips", async () => {
    let s = await snapshot();
    s = await act({type: "new", name: "Path editing", width: 32, height: 24}, s.revision);
    const original = item();
    original.path_style = {...defaultPathStyle(), smoothness: 1, transition_out: "shrink"};
    s = await act({type: "add", level: 0, layer: 1, item: original}, s.revision);
    const project = structuredClone(s.project), edited = project.levels[0].layers[1].items[0];
    moveVertex(edited, {ring: 0, point: 1}, {x: 6, y: 3});
    insertVertex(edited, {ring: 0, point: 2}, {x: 9, y: 5});
    assert.ok(validGeometry(edited));
    setPathTexture(edited, {id: "road", size: {x: 3, y: 2}} as AssetMeta);
    s = await act({type: "commit", project}, s.revision);
    assert.deepEqual(s.project.levels[0].layers[1].items[0], edited);
    s = await act({type: "undo"}, s.revision);
    assert.deepEqual(s.project.levels[0].layers[1].items[0], original);
    s = await act({type: "redo"}, s.revision);
    assert.deepEqual(JSON.parse(JSON.stringify(s.project)).levels[0].layers[1].items[0], edited);
});
