import {test} from "node:test";
import assert from "node:assert/strict";
import {ThumbnailQueue} from "../src/components/thumbnailQueue";

const tick = () => new Promise<void>(resolve => setImmediate(resolve));

function deferred() {
    let resolve!: (value: string) => void, reject!: (error: unknown) => void;
    const promise = new Promise<string>((yes, no) => {
        resolve = yes;
        reject = no;
    });
    return {promise, resolve, reject};
}

test("switching pages discards unmounted requests and keeps four concurrent reads", async () => {
    const queue = new ThumbnailQueue();
    const started: string[] = [], reads = new Map<string, ReturnType<typeof deferred>>();
    const load = (id: string) => queue.request(id, () => {
        started.push(id);
        const read = deferred();
        reads.set(id, read);
        return read.promise;
    });
    const old = Array.from({length: 48}, (_, n) => load(`old-${n}`));
    await tick();
    assert.equal(started.length, 4);
    old.forEach(request => request.release());
    const current = load("current");
    reads.get("old-0")!.resolve("old");
    await tick();
    assert.deepEqual(started, ["old-0", "old-1", "old-2", "old-3", "current"]);
    reads.get("current")!.resolve("current image");
    assert.equal(await current.promise, "current image");
    for (const read of reads.values()) read.resolve("done");
    await Promise.all(old.map(request => request.promise));
});

test("catalog resets discard old backlog without allowing late results to replace new images", async () => {
    const queue = new ThumbnailQueue(1);
    const old = deferred(), fresh = deferred();
    const first = queue.request("same", () => old.promise);
    const stale = queue.request("stale", () => {
        throw Error("stale request ran");
    });
    queue.reset();
    const replacement = queue.request("same", () => fresh.promise);
    old.resolve("old catalog");
    assert.equal(await stale.promise, "");
    assert.equal(await first.promise, "old catalog");
    await tick();
    fresh.resolve("new catalog");
    assert.equal(await replacement.promise, "new catalog");
    await tick();
    assert.equal(await queue.request("same", () => {
        throw Error("lost cache");
    }).promise, "new catalog");
});

test("shared cards retain their queued request until their final subscriber leaves", async () => {
    const queue = new ThumbnailQueue(1);
    const busy = deferred();
    queue.request("busy", () => busy.promise);
    let reads = 0;
    const first = queue.request("shared", async () => {
        reads++;
        return "image";
    });
    const second = queue.request("shared", async () => {
        throw Error("duplicate");
    });
    first.release();
    first.release();
    busy.resolve("done");
    assert.equal(await second.promise, "image");
    assert.equal(reads, 1);
});

test("a failed read is retryable and does not block following thumbnails", async () => {
    const queue = new ThumbnailQueue(1);
    const first = queue.request("asset", async () => {
        throw Error("offline");
    });
    await assert.rejects(first.promise, /offline/);
    assert.equal(await queue.request("asset", async () => "reconnected").promise, "reconnected");
    assert.equal(await queue.request("next", async () => "next").promise, "next");
});

test("bounded cache evicts finished unused entries without evicting an active read", async () => {
    const queue = new ThumbnailQueue(2, 1);
    const slow = deferred();
    const running = queue.request("slow", () => slow.promise);
    for (let n = 0; n < 10; n++) {
        const request = queue.request(`${n}`, async () => `${n}`);
        request.release();
        await request.promise;
        await tick();
    }
    assert.equal(queue.request("slow", async () => "duplicate").promise, running.promise);
    slow.resolve("slow image");
    assert.equal(await running.promise, "slow image");
});
