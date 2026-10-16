import {test} from "node:test";
import assert from "node:assert/strict";
import {newRoof, roofGeometry, roofQuickbox, roofStyle, roofStyleAsset, roofStyleName} from "../src/tools/roofs";
import {type Asset, bounds, hit, newItem, translate} from "../src/map/model";
import {
    geometryRings,
    insertVertex,
    moveVertex,
    removeVertex,
    signedArea,
    validGeometry
} from "../src/tools/architecture";

const asset = (part: string): Asset => ({
    category: "roofs",
    builtin: true,
    meta: {
        id: `res://textures/roofs/red_clay/${part}.png`,
        name: part,
        path: "",
        tags: [],
        size: {x: 4, y: 4},
        pivot: {x: 0.5, y: 0.5},
        sha256: "",
        color_mode: "none"
    }
});
const assets = ["tiles", "ridge", "edge", "hip"].map(asset);
const roof = (type: "gable" | "hip" | "dormer" = "gable") => newItem("Roof", newRoof([{x: 2, y: 4}, {
    x: 12,
    y: 4
}], 4, type, assets[0], assets), assets[0].meta.id);

test("quickbox follows the long axis in every drag direction", () => {
    assert.deepEqual(roofQuickbox({x: 12, y: 6}, {x: 2, y: 2}), {points: [{x: 2, y: 4}, {x: 12, y: 4}], width: 4});
    assert.deepEqual(roofQuickbox({x: 2, y: 2}, {x: 6, y: 12}), {points: [{x: 4, y: 2}, {x: 4, y: 12}], width: 4});
});
test("roof library presents styles and resolves each set of companion textures", () => {
    assert.deepEqual(assets.filter(roofStyleAsset), [assets[0]]);
    assert.equal(roofStyleName(assets[0]), "red clay");
    assert.equal(roofStyle(assets[0], assets).ridge, assets[1].meta.id);
    assert.equal(roofStyle(assets[0], [assets[0]]).ridge, null);
});
test("three roof types have different facets and footprints", () => {
    const g = roofGeometry((roof().kind as ReturnType<typeof newRoof>));
    const h = roofGeometry((roof("hip").kind as ReturnType<typeof newRoof>));
    const d = roofGeometry((roof("dormer").kind as ReturnType<typeof newRoof>));
    assert.equal(g.facets.length, 2);
    assert.equal(h.facets.length, 4);
    assert.equal(h.hips.length, 4);
    assert.deepEqual(h.ridge, [{x: 4, y: 4}, {x: 10, y: 4}]);
    assert.equal(d.outline.length, 5);
    assert.equal(Math.abs(signedArea(g.outline)), 40);
    assert.equal(Math.abs(signedArea(d.outline)), 36);
});
test("selection follows eaves and dormer tip instead of just the ridge", () => {
    const r = roof();
    assert.ok(hit(r, {x: 3, y: 5.9}, 0));
    assert.ok(!hit(r, {x: 3, y: 6.1}, 0));
    assert.deepEqual(bounds(r), {x: 2, y: 2, right: 12, bottom: 6});
    const d = roof("dormer");
    assert.ok(!hit(d, {x: 11.8, y: 5.9}, 0));
    assert.ok(hit(d, {x: 11.8, y: 4}, 0));
    translate(r, {x: 5, y: 7});
    assert.deepEqual(bounds(r), {x: 7, y: 9, right: 17, bottom: 13});
});
test("manual zigzags join slopes without gaps or overlap and allow point editing", () => {
    const r = roof();
    if (r.kind.type !== "roof") throw Error();
    r.kind.points = [{x: 2, y: 4}, {x: 10, y: 2}, {x: 16, y: 16}, {x: 24, y: 12}, {x: 22, y: 5}];
    r.kind.width = 3;
    assert.ok(validGeometry(r));
    const g = roofGeometry(r.kind), area = g.facets.reduce((s, f) => s + Math.abs(signedArea(f.points)), 0);
    assert.ok(Math.abs(area - Math.abs(signedArea(g.outline))) < 1e-7);
    assert.equal(g.ridge.length, 5);
    assert.equal(g.facets.length, 8);
    insertVertex(r, {ring: 0, point: 1}, {x: 6, y: 3});
    assert.equal(geometryRings(r)[0].length, 6);
    moveVertex(r, {ring: 0, point: 1}, {x: 6, y: 2.8});
    assert.ok(validGeometry(r));
    assert.ok(removeVertex(r, {ring: 0, point: 1}));
    assert.equal(geometryRings(r)[0].length, 5);
});
test("reject degenerate and folded manual roofs", () => {
    const r = roof();
    if (r.kind.type !== "roof") throw Error();
    r.kind.width = 0;
    assert.ok(!validGeometry(r));
    r.kind.width = 4;
    r.kind.points = [{x: 2, y: 4}, {x: 2, y: 4}];
    assert.ok(!validGeometry(r));
    r.kind.points = [{x: 2, y: 4}, {x: 8, y: 4}, {x: 2, y: 4}];
    assert.ok(!validGeometry(r));
});
