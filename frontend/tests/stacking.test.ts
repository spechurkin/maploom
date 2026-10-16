import {test} from 'node:test';
import assert from 'node:assert/strict';
import {moveToLayer, newItem, newLevel, pick, reorderItems} from '../src/map/model';

const tile = (name: string) => newItem(name, {
    type: 'floor',
    color: [255, 255, 255, 255],
    points: [{x: 0, y: 0}, {x: 4, y: 0}, {x: 4, y: 4}, {x: 0, y: 4}]
});
test('layer transfers preserve IDs, texture scale, relative order, and topmost picking', () => {
    const level = newLevel('test');
    const [a, b, c] = ['a', 'b', 'c'].map(tile);
    a.texture_scale = 8;
    level.layers[0].items = [a, b];
    level.layers[1].items = [c];
    moveToLayer(level, [a.id, b.id], level.layers[1].id);
    assert.deepEqual(level.layers[0].items, []);
    assert.deepEqual(level.layers[1].items, [c, a, b]);
    assert.equal(a.texture_scale, 8);
    assert.equal(pick(level, {x: 2, y: 2}, 0.1)?.id, b.id);
    const restored = JSON.parse(JSON.stringify(level));
    assert.deepEqual(restored.layers[1].items.map((i: { id: string }) => i.id), [c.id, a.id, b.id]);
});
test('layer transfers skip locked sources and items and reject hidden or locked targets', () => {
    const level = newLevel('test');
    const [a, b, c] = ['a', 'b', 'c'].map(tile);
    b.locked = true;
    level.layers[0].items = [a, b];
    level.layers[1].items = [c];
    level.layers[1].locked = true;
    for (const target of [level.layers[2]]) {
        target.locked = true;
        moveToLayer(level, [a.id], target.id);
        assert.deepEqual(level.layers[0].items, [a, b]);
        target.locked = false;
        target.visible = false;
        moveToLayer(level, [a.id], target.id);
        assert.deepEqual(level.layers[0].items, [a, b]);
        target.visible = true;
    }
    moveToLayer(level, [a.id, b.id, c.id], level.layers[2].id);
    assert.deepEqual(level.layers[2].items, [a]);
    assert.deepEqual(level.layers[0].items, [b]);
    assert.deepEqual(level.layers[1].items, [c]);
});
test('stack controls preserve group order and agree with hit testing', () => {
    const level = newLevel('test');
    const [a, b, c, d] = ['a', 'b', 'c', 'd'].map(tile);
    const l = level.layers[0];
    l.items = [a, b, c, d];
    reorderItems(level, [a.id, b.id], 'up');
    assert.deepEqual(l.items, [c, a, b, d]);
    reorderItems(level, [a.id, b.id], 'down');
    assert.deepEqual(l.items, [a, b, c, d]);
    reorderItems(level, [a.id, c.id], 'front');
    assert.deepEqual(l.items, [b, d, a, c]);
    assert.equal(pick(level, {x: 2, y: 2}, 0.1)?.id, c.id);
    reorderItems(level, [a.id, c.id], 'back');
    assert.deepEqual(l.items, [a, c, b, d]);
    a.locked = true;
    reorderItems(level, [a.id], 'front');
    assert.deepEqual(l.items, [a, c, b, d]);
    l.locked = true;
    reorderItems(level, [d.id], 'back');
    assert.deepEqual(l.items, [a, c, b, d]);
});
