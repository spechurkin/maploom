import {test} from "node:test";
import assert from "node:assert/strict";
import {
    type AssetMeta,
    bounds,
    boxSelection,
    corners,
    hit,
    newItem,
    newLevel,
    pick,
    portalType,
    setPortalStyle
} from "../src/map/model";
import {type Overlay, overlayGeometry} from "../src/rendering/gpu";

const meta: AssetMeta = {
    id: "portal", name: "portal", path: "portal.png", tags: [],
    size: {x: 2, y: 1}, pivot: {x: 0.5, y: 0.5}, sha256: "", color_mode: "none"
};
const assets = {portal: meta};

function layeredRoom() {
    const level = newLevel("Test");
    const room = newItem("Room", {
        type: "room", rings: [[{x: 0, y: 0}, {x: 10, y: 0}, {x: 10, y: 10}, {x: 0, y: 10}]],
        floor: null, wall: null, width: 0.2
    });
    const object = (x: number) => newItem("Object", {
        type: "object", asset: "object", position: {x, y: 4}, size: {x: 1, y: 1},
        pivot: {x: 0.5, y: 0.5}, rotation: 0, flip_x: false, flip_y: false
    });
    const first = object(4), second = object(7);
    level.layers[1].items.push(room);
    level.layers[2].items.push(first, second);
    return {level, room, first, second};
}

test("selection on Objects starts a box inside a room and never picks another layer", () => {
    const {level, room, first, second} = layeredRoom();
    const objects = level.layers[2], architecture = level.layers[1];
    assert.equal(pick(level, {x: 2, y: 2}, 0)?.id, room.id);
    assert.equal(pick(level, {x: 2, y: 2}, 0, {}, objects.id), undefined);
    assert.equal(pick(level, {x: 4, y: 4}, 0, {}, objects.id)?.id, first.id);
    assert.equal(pick(level, {x: 4, y: 4}, 0, {}, architecture.id)?.id, room.id);
    // Both directions of the drag select only the two objects inside the room.
    for (const [a, b] of [[{x: 2, y: 2}, {x: 9, y: 6}], [{x: 9, y: 6}, {x: 2, y: 2}]]) {
        assert.deepEqual(boxSelection(objects, a, b), [first.id, second.id]);
        assert.deepEqual(boxSelection(architecture, a, b), [room.id]);
    }
    assert.equal(pick(level, {x: 4, y: 4}, 0, {}, "missing-layer"), undefined);
});

test("Shift box selection preserves only the active layer and removes duplicates", () => {
    const {level, room, first, second} = layeredRoom();
    const objects = level.layers[2];
    assert.deepEqual(boxSelection(objects, {x: 6, y: 3}, {x: 8, y: 5}, {},
        [room.id, first.id, second.id, "deleted-item"]), [first.id, second.id]);
    assert.deepEqual(boxSelection(objects, {x: 8, y: 8}, {x: 9, y: 9}, {},
        [room.id, first.id]), [first.id]);
    objects.visible = false;
    assert.equal(pick(level, {x: 4, y: 4}, 0, {}, objects.id), undefined);
    assert.deepEqual(boxSelection(objects, {x: 0, y: 0}, {x: 10, y: 10}, {}, [first.id]), []);
});

test("light picking covers its range over enclosed objects and respects the active layer and stack", () => {
    const {level, first} = layeredRoom();
    const architecture = level.layers[1], objects = level.layers[2];
    const light = newItem("Light", {
        type: "light", position: {x: 5, y: 5}, range: 4,
        intensity: 1, color: [255, 255, 255, 255], shadows: true
    });
    architecture.items.push(light);
    // The cursor is over an object on another layer, far from the light's center.
    const p = {x: 4, y: 4};
    assert.equal(pick(level, p, 0, {}, architecture.id)?.id, light.id);
    assert.equal(pick(level, p, 0, {}, objects.id)?.id, first.id);
    assert.equal(pick(level, p, 0)?.id, first.id);
    objects.items.push(light);
    architecture.items.pop();
    assert.equal(pick(level, p, 0, {}, objects.id)?.id, light.id);
    objects.items.reverse();
    assert.equal(pick(level, p, 0, {}, objects.id)?.id, first.id);
    architecture.visible = false;
    assert.equal(pick(level, p, 0, {}, architecture.id), undefined);
});

test("light picking follows the circular range and retains a target for tiny lights", () => {
    const light = newItem("Light", {
        type: "light", position: {x: 5.23, y: 5.23}, range: 4,
        intensity: 1, color: [255, 255, 255, 255], shadows: true
    });
    for (const range of [0, 0.1, 0.3, 2, 4]) {
        if (light.kind.type !== "light") throw Error("Expected light");
        light.kind.range = range;
        const radius = Math.max(0.3, range);
        assert.equal(hit(light, {x: 5.23 + radius - 0.001, y: 5.23}, 0), true);
        assert.equal(hit(light, {x: 5.23 + radius + 0.05, y: 5.23}, 0), false);
        assert.equal(hit(light, {x: 5.23 + radius + 0.05, y: 5.23}, 0.1), true);
        assert.equal(hit(light, {x: 5.23 + radius, y: 5.23 + radius}, 0), false);
    }
});

test("restyling placed portals preserves geometry, attachment, color and light blocking", () => {
    for (const type of ["door", "window"] as const) for (const legacy of [false, true]) {
        const previous = {...meta, id: `old-${type}`, name: type};
        const next = {
            ...meta, id: type === "door" ? "window" : "door", name: type === "door" ? "window" : "door",
            size: {x: 10, y: 4}
        };
        const item = newItem("Custom name", {
            type: "portal", bounds: [{x: 4, y: 2}, {x: 6, y: 5}],
            closed: type === "door", color: [30, 60, 90, 180]
        }, previous.id);
        item.wall_attachment = {wall: "wall", segment: 3, t: 0.4, length: Math.sqrt(13)};
        item.tint = [90, 80, 70, 255];
        if (!legacy) item.portal_type = type;
        const before = structuredClone(item);
        setPortalStyle(item, next, previous);
        assert.deepEqual(item, {...before, texture: next.id, portal_type: type});
        assert.equal(portalType(item, next), type);
        assert.deepEqual(JSON.parse(JSON.stringify(item)), item);
    }
});

test("restyling a freestanding portal keeps its bounds and legacy fallback without metadata", () => {
    const item = newItem("Opening", {
        type: "portal", bounds: [{x: 2, y: 3}, {x: 5, y: 3}],
        closed: false, color: [255, 255, 255, 255]
    }, "missing");
    const before = structuredClone(item);
    setPortalStyle(item, {...meta, id: "door", name: "Door"});
    assert.deepEqual(item, {...before, texture: "door", portal_type: "window"});
});

test("portal picking covers its rendered thickness at every angle and opening length", () => {
    for (const angle of [0, Math.PI / 2, Math.PI, -Math.PI / 2, 0.7]) {
        for (const length of [1, 2, 5]) {
            const at = (x: number, y: number) => ({
                x: 8 + x * Math.cos(angle) - y * Math.sin(angle),
                y: 6 + x * Math.sin(angle) + y * Math.cos(angle)
            });
            const item = newItem("Opening", {
                type: "portal", bounds: [at(-length / 2, 0), at(length / 2, 0)],
                closed: true, color: [255, 255, 255, 255]
            }, "portal");
            const level = newLevel("Test");
            level.layers[1].items.push(item);
            for (const sign of [-1, 1]) {
                assert.equal(pick(level, at(0, sign * length * 0.24), 0, assets)?.id, item.id);
                assert.equal(hit(item, at(0, sign * (length / 4 + 0.06)), 0.02, assets), false);
                assert.equal(hit(item, at(sign * (length / 2 + 0.06), 0), 0.02, assets), false);
            }
            const b = bounds(item, assets);
            assert.ok(Math.abs((b.x + b.right) / 2 - 8) < 1e-9);
            assert.ok(Math.abs((b.y + b.bottom) / 2 - 6) < 1e-9);
            const copy = JSON.parse(JSON.stringify(item));
            copy.kind.bounds.reverse();
            assert.deepEqual(bounds(copy, assets), b);
        }
    }
});

test("missing portal textures retain line picking and can still be selected", () => {
    const item = newItem("Opening", {
        type: "portal", bounds: [{x: 2, y: 1}, {x: 2, y: 4}],
        closed: false, color: [255, 255, 255, 255]
    }, "missing");
    assert.equal(hit(item, {x: 2.05, y: 2}, 0.02, assets), true);
    assert.equal(hit(item, {x: 2.3, y: 2}, 0.02, assets), false);
    assert.equal(corners(item, assets).length, 2);
});

test("selection outlines center minimum dimensions and follow the sprite while dragging", () => {
    for (const texture of [null, "portal"]) {
        const item = newItem("Opening", {
            type: "portal", bounds: [{x: 4, y: 2}, {x: 4, y: 5}],
            closed: true, color: [255, 255, 255, 255]
        }, texture);
        const overlay: Overlay = {items: [item], assets, gesture: [], drag: null, radius: 1, accent: [20, 160, 80]};
        for (const scale of [4, 40, 200]) {
            for (const move of [false, true]) {
                overlay.drag = move ? {mode: "move", start: {x: 1, y: 2}, last: {x: 4, y: 6}} : null;
                // Four outline edges precede the corner handles. Their outer bounds
                // must remain symmetric around the visual center, including tiny items.
                const {vertices} = overlayGeometry(overlay, scale);
                const xs: number[] = [], ys: number[] = [];
                for (let n = 0; n < 16; n++) {
                    xs.push(vertices[n * 12]);
                    ys.push(vertices[n * 12 + 1]);
                }
                assert.ok(Math.abs((Math.min(...xs) + Math.max(...xs)) / 2 - (move ? 7 : 4)) < 1e-9);
                assert.ok(Math.abs((Math.min(...ys) + Math.max(...ys)) / 2 - (move ? 7.5 : 3.5)) < 1e-9);
                const b = bounds(item, assets);
                assert.ok(Math.max(...xs) - Math.min(...xs) >= b.right - b.x);
                assert.ok(Math.max(...ys) - Math.min(...ys) >= b.bottom - b.y);
            }
        }
    }
});

test("object picking retains off-center pivots, rotation and both mirror flags", () => {
    for (const flip_x of [false, true]) for (const flip_y of [false, true]) {
        const item = newItem("Object", {
            type: "object", asset: "object", position: {x: 8, y: 6},
            size: {x: 4, y: 2}, pivot: {x: 0, y: 1}, rotation: Math.PI / 2, flip_x, flip_y
        });
        assert.equal(hit(item, {x: 9, y: 8}, 0, assets), true);
        assert.equal(hit(item, {x: 7, y: 8}, 0, assets), false);
        const b = bounds(item, assets);
        assert.ok(Math.abs(b.x - 8) < 1e-9 && Math.abs(b.right - 10) < 1e-9);
        assert.ok(Math.abs(b.y - 6) < 1e-9 && Math.abs(b.bottom - 10) < 1e-9);
    }
});
