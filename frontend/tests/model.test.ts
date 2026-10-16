import {test} from "node:test";
import assert from "node:assert/strict";
import {
    bounds,
    hex,
    hit,
    newItem,
    newLevel,
    pick,
    portalType,
    rgba,
    roomTint,
    setRoomTint,
    translate,
} from "../src/map/model";

test("portal type handles pack names and lets manual selection override automatic door priority", () => {
    const item = newItem("Renamed opening", {
        type: "portal", bounds: [{x: 0, y: 0}, {x: 1, y: 0}], closed: true, color: [255, 255, 255, 255],
    });
    for (const name of ["BlueLinedLongWindow.webp", "window1.png", "TWindowNarrow01.png", "Windows-07.png"]) {
        item.texture = `res://packs/id/textures/portals/${name}`;
        assert.equal(portalType(item), "window", name);
    }
    for (const name of ["WindowDoor.png", "window_door_01.png", "WINDOWDOOR.png", "Окно-дверь.png"]) {
        item.texture = name;
        item.kind.type === "portal" && (item.kind.closed = false);
        assert.equal(portalType(item), "door", name);
    }
    item.portal_type = "window";
    assert.equal(portalType(JSON.parse(JSON.stringify(item))), "window");
    item.portal_type = "door";
    item.texture = "window.png";
    assert.equal(portalType(item), "door");
    item.portal_type = undefined;
    item.texture = "abc123.png";
    const meta = {
        id: "abc123.png",
        name: "abc123",
        path: "abc123.png",
        tags: ["[Pack] Windows"],
        size: {x: 1, y: 1},
        pivot: {x: 0.5, y: 0.5},
        sha256: "",
        color_mode: "none" as const
    };
    assert.equal(portalType(item, meta), "window");
    assert.equal(portalType(item, {...meta, name: "WindowDoor"}), "door");
});

test("room colors migrate a shared tint without changing the other part", () => {
    const room = newItem("Room", {
        type: "room",
        rings: [
            [
                {x: 0, y: 0},
                {x: 3, y: 0},
                {x: 3, y: 3},
            ],
        ],
        floor: "floor",
        wall: "wall",
        width: 0.2,
    });
    room.tint = [80, 120, 160, 255];
    room.floor_tint = [150, 180, 210, 255];
    const originalWall = roomTint(room, false);
    setRoomTint(room, true, [160, 100, 50, 255]);
    assert.deepEqual(roomTint(room, true), [160, 100, 50, 255]);
    assert.deepEqual(roomTint(room, false), originalWall);
    assert.deepEqual(room.tint, [255, 255, 255, 255]);
    setRoomTint(room, false, [20, 60, 220, 255]);
    assert.deepEqual(roomTint(room, true), [160, 100, 50, 255]);
    assert.deepEqual(roomTint(room, false), [20, 60, 220, 255]);
});
test("rotated object hit testing respects the pivot and transformed corners", () => {
    const i = newItem("object", {
        type: "object",
        asset: "a",
        position: {x: 4, y: 5},
        size: {x: 4, y: 1},
        rotation: Math.PI / 2,
        pivot: {x: 0.5, y: 0.5},
        flip_x: false,
        flip_y: false,
    });
    assert.equal(hit(i, {x: 4, y: 6}, 0.1), true);
    assert.equal(hit(i, {x: 5, y: 5}, 0.1), false);
    const b = bounds(i);
    assert.ok(Math.abs(b.x - 3.5) < 0.001);
    assert.ok(Math.abs(b.bottom - 7) < 0.001);
});
test("room holes are not selectable and hidden layers do not intercept selection", () => {
    const i = newItem("room", {
        type: "room",
        floor: null,
        wall: null,
        width: 0.2,
        rings: [
            [
                {x: 0, y: 0},
                {x: 8, y: 0},
                {x: 8, y: 8},
                {x: 0, y: 8},
            ],
            [
                {x: 2, y: 2},
                {x: 4, y: 2},
                {x: 4, y: 4},
                {x: 2, y: 4},
            ],
        ],
    });
    assert.equal(hit(i, {x: 3, y: 3}, 0.1), false);
    assert.equal(hit(i, {x: 1, y: 1}, 0.1), true);
    const l = newLevel("test");
    l.layers[0].items = [i];
    assert.equal(pick(l, {x: 1, y: 1}, 0.1)?.id, i.id);
    l.layers[0].visible = false;
    assert.equal(pick(l, {x: 1, y: 1}, 0.1), undefined);
});
test("portal selection follows its segment and translating geometry preserves the document shape", () => {
    const i = newItem("portal", {
        type: "portal",
        bounds: [
            {x: 2, y: 2},
            {x: 5, y: 5},
        ],
        color: [255, 255, 255, 255],
        closed: true,
    });
    assert.equal(hit(i, {x: 3, y: 3}, 0.1), true);
    assert.equal(hit(i, {x: 2, y: 5}, 0.1), false);
    translate(i, {x: 1, y: -1});
    if (i.kind.type === "portal")
        assert.deepEqual(i.kind.bounds, [
            {x: 3, y: 1},
            {x: 6, y: 4},
        ]);
    assert.equal(hex(rgba("#12a5ff")), "#12a5ff");
});
