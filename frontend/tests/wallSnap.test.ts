import test from "node:test";
import assert from "node:assert/strict";
import {type Item, newItem, newLevel, type Point} from "../src/map/model";
import {objectSilhouette, snapObjectsToWalls, snapWalls, type WallSegment} from "../src/tools/wallSnap";
import {circleContour} from "../src/tools/architecture";

const rect = (x: number, y: number, right: number, bottom: number): Point[] =>
    [{x, y}, {x: right, y}, {x: right, y: bottom}, {x, y: bottom}];
const hull = rect(.2, .25, .8, .75);
const object = (x = 2, y = 1): Item => newItem("Globe", {
    type: "object", asset: "globe", position: {x, y},
    size: {x: 4, y: 2}, pivot: {x: .5, y: .5}, rotation: 0, flip_x: false, flip_y: false
});
const horizontal: WallSegment = {a: {x: -10, y: 0}, b: {x: 10, y: 0}, width: .2};
const silhouettes = new Map([["globe", hull]]);
const near = (a: number, b: number) => assert.ok(Math.abs(a - b) < 1e-7, `${a} != ${b}`);

test("alpha outline meets the wall face plus five map pixels, ignoring transparent margins", () => {
    const i = object(2, .78);
    const delta = snapObjectsToWalls([i], {x: 0, y: 0}, [horizontal], silhouettes, 100, .3)!;
    near(delta.x, 0);
    near(delta.y, -.13);
    near(Math.min(...objectSilhouette(i, hull, delta).map(p => p.y)), .1 + 5 / 100);
    // The selection box would extend through the wall; it is deliberately unused.
    assert.ok(i.kind.type === "object" && i.kind.position.y + delta.y - i.kind.size.y / 2 < 0);
});

test("both wall sides, reversed endpoints and map resolution keep the correct clearance", () => {
    for (const sign of [-1, 1]) for (const ppg of [50, 100, 200]) for (const reverse of [false, true]) {
        const i = object(2, sign * .75),
            wall = reverse ? {...horizontal, a: horizontal.b, b: horizontal.a} : horizontal;
        const delta = snapObjectsToWalls([i], {x: 0, y: 0}, [wall], silhouettes, ppg, .4)!;
        const ys = objectSilhouette(i, hull, delta).map(p => sign * p.y);
        near(Math.min(...ys), wall.width / 2 + 5 / ppg);
    }
});

test("rotation, scale, flips and an off-center pivot use the transformed alpha silhouette", () => {
    for (const angle of [0, Math.PI / 6, Math.PI / 2]) for (const flip of [true, false]) {
        const i = object(0, 0);
        if (i.kind.type !== "object") throw Error();
        i.kind.rotation = angle;
        i.kind.flip_x = flip;
        i.kind.flip_y = flip;
        i.kind.pivot = {x: .1, y: .7};
        i.kind.size = {x: 2, y: 3};
        const asymmetric = rect(.1, .2, .65, .9);
        const min = Math.min(...objectSilhouette(i, asymmetric, {x: 0, y: 0}).map(p => p.y));
        i.kind.position.y = .3 - min;
        const delta = snapObjectsToWalls([i], {x: 0, y: 0}, [horizontal], new Map([["globe", asymmetric]]), 100, .4)!;
        near(Math.min(...objectSilhouette(i, asymmetric, delta).map(p => p.y)), .15);
    }
});

test("oblique walls use the silhouette support rather than its rectangular selection", () => {
    const i = object(0, 0), n = {x: -Math.SQRT1_2, y: Math.SQRT1_2};
    const triangle = [{x: .5, y: .1}, {x: .9, y: .9}, {x: .1, y: .9}];
    const projection = (p: Point) => p.x * n.x + p.y * n.y;
    const min = Math.min(...objectSilhouette(i, triangle, {x: 0, y: 0}).map(projection));
    if (i.kind.type !== "object") throw Error();
    i.kind.position = {x: n.x * (.3 - min), y: n.y * (.3 - min)};
    const delta = snapObjectsToWalls([i], {x: 0, y: 0}, [{a: {x: -10, y: -10}, b: {x: 10, y: 10}, width: .2}],
        new Map([["globe", triangle]]), 100, .4)!;
    near(Math.min(...objectSilhouette(i, triangle, delta).map(projection)), .15);
});

test("corners satisfy both wall faces; a group keeps its internal offsets", () => {
    const vertical = {a: {x: 0, y: -10}, b: {x: 0, y: 10}, width: .2};
    const i = object(1.5, .8),
        delta = snapObjectsToWalls([i], {x: 0, y: 0}, [horizontal, vertical], silhouettes, 100, .4)!;
    const ps = objectSilhouette(i, hull, delta);
    near(Math.min(...ps.map(p => p.x)), .15);
    near(Math.min(...ps.map(p => p.y)), .15);
    const group = [object(2, .8), object(5, 1.2)];
    const shift = snapObjectsToWalls(group, {x: .5, y: 0}, [horizontal], silhouettes, 100, .3)!;
    near(shift.x, .5);
    near(shift.y, -.15);
});

test("far walls, wall extensions, empty or missing alpha and nonobjects do not attract", () => {
    assert.equal(snapObjectsToWalls([object(2, 5)], {x: 0, y: 0}, [horizontal], silhouettes, 100, .3), null);
    assert.equal(snapObjectsToWalls([object(20, .7)], {x: 0, y: 0}, [horizontal], silhouettes, 100, .3), null);
    for (const hulls of [new Map(), new Map([["globe", []]])])
        assert.equal(snapObjectsToWalls([object()], {x: 0, y: 0}, [horizontal], hulls, 100, 1), null);
    assert.equal(snapObjectsToWalls([newItem("Light", {
        type: "light", position: {x: 0, y: 0}, range: 2,
        intensity: 1, color: [255, 255, 255, 255], shadows: true
    })], {x: 0, y: 0}, [horizontal], silhouettes, 100, 1), null);
});

test("visible room contours and cutout walls respect exclusions, hidden layers, paths and openings", () => {
    const level = newLevel("Level");
    const room = newItem("Room", {
        type: "room",
        rings: [rect(0, 0, 10, 10), rect(2, 2, 4, 4)],
        wall: "stone",
        floor: null,
        width: .3
    });
    room.wall_exclusions = [1];
    level.layers[1].locked = true;
    level.layers[1].items.push(room);
    assert.equal(snapWalls(level).length, 4); // Locked walls still attract.
    room.wall_exclusions = [];
    assert.equal(snapWalls(level).length, 8);
    room.wall_exclusions = [0, 1];
    assert.equal(snapWalls(level).length, 0);
    room.wall_exclusions = [1];
    level.layers[0].items.push(newItem("Door", {
        type: "portal",
        bounds: [{x: 3, y: 0}, {x: 5, y: 0}],
        closed: true,
        color: [255, 255, 255, 255]
    }));
    const walls = snapWalls(level);
    assert.equal(walls.length, 5);
    assert.deepEqual(walls.slice(0, 2).map(w => [w.a, w.b]), [[{x: 0, y: 0}, {x: 3, y: 0}], [{x: 5, y: 0}, {
        x: 10,
        y: 0
    }]]);
    const doorwayObject = object(4, .7);
    if (doorwayObject.kind.type === "object") doorwayObject.kind.size.x = 1;
    assert.equal(snapObjectsToWalls([doorwayObject], {x: 0, y: 0}, walls, silhouettes, 100, .3), null);
    level.layers[1].visible = false;
    assert.equal(snapWalls(level).length, 0);
    level.layers[1].visible = true;
    assert.equal(snapWalls(level, new Set([room.id])).length, 0);
});

test("curved attached portals cut the full arc across the contour seam, only on their own wall", () => {
    const level = newLevel("Level");
    const ring = circleContour({x: 10, y: 10}, {x: 15, y: 10}, 64);
    const room = newItem("Round room", {type: "room", rings: [ring], wall: "stone", floor: null, width: .3});
    const other = newItem("Other wall", {
        type: "room",
        rings: [ring.map(p => ({x: p.x + .05, y: p.y}))],
        wall: "stone",
        floor: null,
        width: .3
    });
    level.layers[1].items.push(room, other);
    const door = newItem("Window", {
        type: "portal",
        bounds: [ring[500], ring[12]],
        closed: false,
        color: [255, 255, 255, 255]
    });
    door.wall_attachment = {wall: room.id, segment: 0, t: 0, length: 1};
    level.layers[2].items.push(door);
    const walls = snapWalls(level);
    const length = walls.reduce((sum, w) => sum + Math.hypot(w.b.x - w.a.x, w.b.y - w.a.y), 0);
    const edge = Math.hypot(ring[1].x - ring[0].x, ring[1].y - ring[0].y);
    near(length, (512 * 2 - 24) * edge);
    level.layers[2].visible = false;
    assert.equal(snapWalls(level).length, 1024);
});
