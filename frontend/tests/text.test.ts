import {test} from "node:test";
import assert from "node:assert/strict";
import {bounds, defaultTextStyle, hit, newItem, newLevel, pick, type Project, translate} from "../src/map/model";
import {rememberTextBounds} from "../src/tools/text";

test("text selection uses renderer metrics, includes the background and follows moves", () => {
    const i = newItem("Text", {
        type: "text",
        position: {x: 5, y: 6},
        text: "Крепость\nNorth gate",
        size: .32,
        color: [0, 0, 0, 255]
    });
    i.text_style = {...defaultTextStyle(), background: true, border_width: 4, font: "Arial"};
    const level = newLevel("Map");
    level.layers[2].items.push(i);
    rememberTextBounds({levels: [level]} as Project, {[i.id]: {x: 3, y: 5, right: 7, bottom: 7}});
    assert.deepEqual(bounds(i), {x: 3, y: 5, right: 7, bottom: 7});
    assert.equal(pick(level, {x: 3.01, y: 5.01}, 0)?.id, i.id);
    assert.equal(hit(i, {x: 2.99, y: 6}, 0), false);
    translate(i, {x: 10, y: 20});
    assert.deepEqual(bounds(i), {x: 13, y: 25, right: 17, bottom: 27});
    if (i.kind.type === "text") i.kind.text = "X";
    assert.notEqual(bounds(i).right - bounds(i).x, 4, "changed content invalidates cached dimensions");
});

test("multiline fallback accounts for Unicode characters and empty lines", () => {
    const i = newItem("Text", {type: "text", position: {x: 0, y: 0}, text: "AB", size: 1, color: [0, 0, 0, 255]});
    const ascii = bounds(i);
    if (i.kind.type === "text") i.kind.text = "ЯЮ";
    assert.deepEqual(bounds(i), ascii, "Unicode must not count UTF-8 bytes");
    if (i.kind.type === "text") i.kind.text = "ЯЮ\n\n";
    assert.ok(bounds(i).bottom - bounds(i).y > ascii.bottom - ascii.y);
    const before = bounds(i);
    i.text_style = {...defaultTextStyle(), background: true, border_width: 0};
    const after = bounds(i);
    assert.ok(Math.abs(after.right - after.x - (before.right - before.x) - .08) < 1e-8);
});
