import {test} from "node:test";
import assert from "node:assert/strict";
import {drawingPoints} from "../src/tools/drawingGeometry";
import {circleContour, rectangle, validGeometry} from "../src/tools/architecture";
import {newItem} from "../src/map/model";
import {simplifyPath} from "../src/tools/paths";
import {roofQuickbox} from "../src/tools/roofs";

const a = {x: 2, y: 3}, b = {x: 8, y: 7};
test("polygon previews include the live endpoint without changing fixed vertices", () => {
    const gesture = [a, {x: 8, y: 3}];
    for (const tool of ["room", "floor", "wall", "path"] as const) {
        assert.deepEqual(drawingPoints(tool, "polygon", gesture, b, null, 64, 24), [...gesture, b]);
        assert.deepEqual(drawingPoints(tool, "polygon", gesture, gesture[1], null, 64, 24), gesture);
        assert.deepEqual(drawingPoints(tool, "polygon", gesture, null, null, 64, 24), gesture);
    }
    assert.equal(gesture.length, 2);
    for (const tool of ["room", "floor"] as const) assert.deepEqual(
        drawingPoints(tool, "polygon", [...gesture, b], a, null, 64, 24), [...gesture, b]);
});

test("drag previews match committed rectangles and circles, including closed wall seams", () => {
    for (const shape of ["rectangle", "circle"] as const) for (const tool of ["room", "floor", "wall"] as const) {
        const expected = shape === "circle" ? circleContour(a, b, 12) : rectangle(a, b);
        const points = drawingPoints(tool, shape, [], b, {mode: "room", start: a, last: b, trail: []}, 12, 24);
        assert.deepEqual(points, tool === "wall" ? [...expected, expected[0]] : expected);
        assert.ok(validGeometry(newItem("Preview", tool === "room" ? {
                type: "room",
                rings: [points],
                width: .3,
                floor: null,
                wall: null
            }
            : tool === "floor" ? {type: "floor", points, color: [255, 255, 255, 255]}
                : {type: "wall", points, width: .3, color: [255, 255, 255, 255]})));
    }
});

test("freehand previews use the final path simplification at every zoom", () => {
    const trail = [a, {x: 4, y: 3.01}, {x: 6, y: 3}, b];
    for (const scale of [2, 24, 400]) assert.deepEqual(
        drawingPoints("path", "polygon", [], b, {mode: "path", start: a, last: b, trail}, 64, scale),
        simplifyPath(trail, 1.5 / scale));
});

test("idle tools and panning cannot resurrect drawing geometry", () => {
    assert.deepEqual(drawingPoints("floor", "polygon", [], b, null, 64, 24), []);
    assert.deepEqual(drawingPoints("floor", "polygon", [a, b], b, {
        mode: "pan",
        start: a,
        last: b,
        trail: []
    }, 64, 24), []);
});

test("roof previews use the same quickbox and manual ridge as the final gesture", () => {
    for (const [start, end] of [[a, b], [b, a], [a, {x: 4, y: 12}]]) {
        const points = drawingPoints("roof", "rectangle", [], end, {mode: "room", start, last: end, trail: []}, 64, 24);
        assert.deepEqual(roofQuickbox(points[0], points[2]), roofQuickbox(start, end));
    }
    const ridge = [a, {x: 6, y: 3}];
    assert.deepEqual(drawingPoints("roof", "polygon", ridge, b, null, 64, 24), [...ridge, b]);
});
