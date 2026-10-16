import {test} from "node:test";
import assert from "node:assert/strict";
import {shortcutKey} from "../src/app/shortcuts";

test("shortcuts use physical keys in English and Russian layouts", () => {
    for (const [letter, russian] of [["Z", "я"], ["Y", "н"], ["S", "ы"], ["K", "л"], ["B", "и"]]) {
        assert.equal(shortcutKey({code: `Key${letter}`, key: russian}), letter.toLowerCase());
        assert.equal(shortcutKey({code: `Key${letter}`, key: letter}), letter.toLowerCase());
    }
    assert.equal(shortcutKey({code: "Delete", key: "Delete"}), "delete");
    assert.equal(shortcutKey({code: "", key: "s"}), "s");
});
