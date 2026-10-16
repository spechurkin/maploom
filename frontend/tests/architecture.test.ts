import {test} from "node:test";
import assert from "node:assert/strict";
import {bounds, hit, newItem, translate} from "../src/map/model";
import {
    addHole,
    circleContour,
    geometryRings,
    geometryTarget,
    insertVertex,
    moveVertex,
    rectangle,
    removeHole,
    removeVertex,
    resizeFromCorner,
    resizeGeometry,
    setContourWalls,
    signedArea,
    validGeometry
} from "../src/tools/architecture";

const rect = (x = 0, y = 0, w = 10, h = 10) => rectangle({x, y}, {x: x + w, y: y + h});
const floor = () => newItem("Floor", {type: "floor", points: rect(), color: [255, 255, 255, 255]});
const room = () => newItem("Room", {type: "room", rings: [rect()], floor: "floor", wall: "wall", width: 0.2});

test("edge clicks insert anywhere along a contour, within a screen-pixel tolerance at every zoom", () => {
    for (const make of [floor, room]) for (const scale of [4, 24, 200]) {
        const i = make();
        const target = geometryTarget(i, {x: 3.17, y: -1.9 / scale}, scale);
        assert.equal(target?.type, "edge");
        assert.deepEqual(target?.position, {x: 3.17, y: 0});
        assert.equal(target?.point, 1);
        insertVertex(i, target!, target!.position);
        assert.equal(geometryRings(i)[0].length, 5);
        assert.ok(validGeometry(i));
        const closing = geometryTarget(i, {x: -1.9 / scale, y: 6.3}, scale);
        assert.equal(closing?.point, 5);
        assert.deepEqual(closing?.position, {x: 0, y: 6.3});
    }
});

test("nearby clicks project onto diagonal walls and hole edges without inflating tolerance for wall thickness", () => {
    const wall = newItem("Wall", {
        type: "wall",
        points: [{x: 0, y: 0}, {x: 10, y: 10}, {x: 20, y: 0}],
        width: 2,
        color: [255, 255, 255, 255]
    });
    const target = geometryTarget(wall, {x: 3.5 - 0.04, y: 3.5 + 0.04}, 24);
    assert.equal(target?.type, "edge");
    assert.deepEqual(target?.position, {x: 3.5, y: 3.5});
    assert.equal(geometryTarget(wall, {x: 10, y: 0}, 24), null); // No phantom closing edge.
    const i = floor();
    addHole(i, rect(2, 2, 6, 6), false);
    const hole = geometryTarget(i, {x: 4.1, y: 2.05}, 24);
    assert.equal(hole?.ring, 1);
    assert.deepEqual(hole?.position, {x: 4.1, y: 2});
});

test("existing vertices take priority, endpoints cannot be duplicated and distant clicks miss", () => {
    const i = floor();
    assert.equal(geometryTarget(i, {x: 0.1, y: 0.1}, 24)?.type, "vertex");
    assert.equal(geometryTarget(i, {x: 4, y: -1}, 24), null);
    const wall = newItem("Wall", {
        type: "wall",
        points: [{x: 0, y: 0}, {x: 10, y: 0}],
        width: 2,
        color: [255, 255, 255, 255]
    });
    assert.equal(geometryTarget(wall, {x: 0, y: -1}, 24), null);
    assert.equal(geometryTarget(wall, {x: 4, y: 0.8}, 24), null);
});

test("edge insertion has a maximum two-pixel buffer on either side at every zoom", () => {
    for (const scale of [2, 4, 24, 200, 400]) for (const side of [-1, 1]) {
        const i = floor();
        assert.equal(geometryTarget(i, {x: 4, y: side * 2 / scale}, scale)?.type, "edge");
        assert.equal(geometryTarget(i, {x: 4, y: side * 2.01 / scale}, scale), null);
        const wall = newItem("Wall", {
            type: "wall",
            points: [{x: 0, y: 0}, {x: 10, y: 0}],
            width: 4,
            color: [255, 255, 255, 255]
        });
        assert.equal(geometryTarget(wall, {x: 4, y: side * 2 / scale}, scale)?.type, "edge");
        assert.equal(geometryTarget(wall, {x: 4, y: side * 2.01 / scale}, scale), null);
    }
});

test("circle drawing creates regular hexagons and smooth constant-radius circles", () => {
    for (const [value, count] of [[6, 6], [12, 12], [63, 63], [64, 512]]) {
        const ring = circleContour({x: 4, y: 5}, {x: 7, y: 9}, value);
        assert.equal(ring.length, count);
        for (const p of ring) assert.ok(Math.abs(Math.hypot(p.x - 4, p.y - 5) - 5) < 1e-10);
        const lengths = ring.map((p, n) => Math.hypot(p.x - ring[(n + 1) % count].x, p.y - ring[(n + 1) % count].y));
        assert.ok(Math.max(...lengths) - Math.min(...lengths) < 1e-10);
        assert.ok(signedArea(ring) > 0);
        const i = room();
        if (i.kind.type === "room") i.kind.rings = [ring];
        assert.ok(validGeometry(i));
    }
});
test("floor cutouts stay empty after translating, resizing and JSON round trips", () => {
    const i = floor();
    addHole(i, rect(2, 2, 2, 2), false);
    assert.ok(validGeometry(i));
    assert.equal(hit(i, {x: 3, y: 3}, 0), false);
    translate(i, {x: 10, y: 10});
    resizeGeometry(i, 20, 30);
    assert.equal(hit(i, {x: 16, y: 19}, 0), false);
    assert.equal(hit(i, {x: 11, y: 11}, 0), true);
    assert.deepEqual(JSON.parse(JSON.stringify(i)).floor_holes, i.floor_holes);
    assert.deepEqual(bounds(i), {x: 10, y: 10, right: 30, bottom: 40});
});
test("room cutouts have independently switchable walls and stable indices after deletion", () => {
    const i = room();
    addHole(i, rect(1, 1, 2, 2), false);
    addHole(i, rect(6, 6, 2, 2), true);
    assert.deepEqual(i.wall_exclusions, [1]);
    setContourWalls(i, 0, false);
    setContourWalls(i, 2, false);
    removeHole(i, 1);
    assert.deepEqual(i.wall_exclusions, [0, 1]);
    setContourWalls(i, 1, true);
    assert.deepEqual(i.wall_exclusions, [0]);
    assert.ok(validGeometry(i));
});
test("inserted wall vertices can be moved, closed wall endpoints remain connected", () => {
    const ring = rect();
    const i = newItem("Wall", {type: "wall", points: [...ring, {...ring[0]}], width: 0.2, color: [255, 255, 255, 255]});
    insertVertex(i, {ring: 0, point: 1}, {x: 5, y: 0});
    moveVertex(i, {ring: 0, point: 1}, {x: 5, y: 2});
    assert.equal(geometryRings(i)[0].length, 6);
    moveVertex(i, {ring: 0, point: 0}, {x: 1, y: 1});
    assert.deepEqual(geometryRings(i)[0][0], geometryRings(i)[0].at(-1));
    assert.ok(removeVertex(i, {ring: 0, point: 0}));
    assert.deepEqual(geometryRings(i)[0][0], geometryRings(i)[0].at(-1));
    assert.ok(validGeometry(i));
});
test("resize corners anchor the opposite corner and scale cutouts together", () => {
    for (let corner = 0; corner < 4; corner++) {
        const i = floor();
        addHole(i, rect(2, 2, 2, 2), false);
        const anchors = rect();
        const fixed = anchors[(corner + 2) % 4];
        resizeFromCorner(i, corner, {x: corner === 0 || corner === 3 ? -10 : 20, y: corner < 2 ? -10 : 20});
        assert.deepEqual(geometryRings(i)[0][(corner + 2) % 4], fixed);
        assert.ok(validGeometry(i));
        assert.equal(bounds(i).right - bounds(i).x, 20);
    }
});
test("cutouts cannot touch, overlap, leave the perimeter or cross a concave boundary", () => {
    for (const r of [rect(-1, 2, 2, 2), rect(0, 2, 2, 2), rect(9, 9, 2, 2), rect(0, 0, 10, 10)]) {
        const i = floor();
        addHole(i, r, false);
        assert.equal(validGeometry(i), false);
    }
    const i = floor();
    addHole(i, rect(2, 2, 4, 4), false);
    addHole(i, rect(3, 3, 1, 1), false);
    assert.equal(validGeometry(i), false);
    const concave = floor();
    if (concave.kind.type === "floor") concave.kind.points = [{x: 0, y: 0}, {x: 10, y: 0}, {x: 10, y: 10}, {
        x: 6,
        y: 10
    }, {x: 6, y: 4}, {x: 4, y: 4}, {x: 4, y: 10}, {x: 0, y: 10}];
    addHole(concave, rect(2, 2, 6, 4), false);
    assert.equal(validGeometry(concave), false);
});
test("self intersections and degenerate polygons are rejected, minimum vertices cannot be deleted", () => {
    const i = floor();
    moveVertex(i, {ring: 0, point: 1}, {x: -2, y: 5});
    assert.equal(validGeometry(i), false);
    const triangle = newItem("Floor", {
        type: "floor",
        points: [{x: 0, y: 0}, {x: 3, y: 0}, {x: 0, y: 3}],
        color: [255, 255, 255, 255]
    });
    assert.equal(removeVertex(triangle, {ring: 0, point: 1}), false);
    const line = newItem("Wall", {
        type: "wall",
        points: [{x: 0, y: 0}, {x: 1, y: 0}],
        width: 0.2,
        color: [255, 255, 255, 255]
    });
    assert.equal(removeVertex(line, {ring: 0, point: 0}), false);
});
