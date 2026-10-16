import {test, type TestContext} from "node:test";
import assert from "node:assert/strict";
import {
    DAY_MS, UPDATE_PERIODS, readLastUpdateCheck, readUpdatePeriod, saveLastUpdateCheck,
    saveUpdatePeriod, scheduleUpdateChecks, updateCheckDelay, updatePeriod
} from "../src/app/updateSchedule";

function mockStorage(t: TestContext, storage: Pick<Storage, "getItem" | "setItem">) {
    const previous = Object.getOwnPropertyDescriptor(globalThis, "localStorage");
    Object.defineProperty(globalThis, "localStorage", {configurable: true, value: storage});
    t.after(() => {
        if (previous) Object.defineProperty(globalThis, "localStorage", previous);
        else Reflect.deleteProperty(globalThis, "localStorage");
    });
}

test("only day, week and month are accepted; missing or old values default to a day", () => {
    assert.deepEqual(UPDATE_PERIODS, ["day", "week", "month"]);
    for (const value of [null, undefined, "hour", "", "year", 6]) assert.equal(updatePeriod(value), "day");
    for (const period of UPDATE_PERIODS) assert.equal(updatePeriod(period), period);
});

test("the chosen period and check time survive reloading settings", t => {
    const values = new Map<string, string>();
    mockStorage(t, {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value)
    });
    assert.equal(readUpdatePeriod(), "day");
    assert.equal(readLastUpdateCheck(), 0);
    for (const period of UPDATE_PERIODS) {
        saveUpdatePeriod(period);
        saveLastUpdateCheck(100_000);
        assert.equal(readUpdatePeriod(), period);
        assert.equal(readLastUpdateCheck(), 100_000);
    }
    values.set("maploom:update-last-check", "invalid");
    assert.equal(readLastUpdateCheck(), 0);
});

test("blocked local storage preserves usable defaults", t => {
    mockStorage(t, {
        getItem: () => { throw Error("Unavailable"); },
        setItem: () => { throw Error("Unavailable"); }
    });
    assert.equal(readUpdatePeriod(), "day");
    assert.equal(readLastUpdateCheck(), 0);
    assert.doesNotThrow(() => saveUpdatePeriod("week"));
    assert.doesNotThrow(() => saveLastUpdateCheck(100_000));
});

test("checks become due exactly after 1, 7 or 30 days", () => {
    const last = 100_000;
    for (const [period, days] of [["day", 1], ["week", 7], ["month", 30]] as const) {
        assert.equal(updateCheckDelay(last, period, last), days * DAY_MS);
        assert.equal(updateCheckDelay(last, period, last + days * DAY_MS - 1), 1);
        assert.equal(updateCheckDelay(last, period, last + days * DAY_MS), 0);
        assert.equal(updateCheckDelay(last, period, last + 31 * DAY_MS), 0);
    }
    assert.equal(updateCheckDelay(0, "day", last), 0);
    assert.equal(updateCheckDelay(last, "day", last - DAY_MS), DAY_MS);
});

for (const [period, days] of [["day", 1], ["week", 7], ["month", 30]] as const) {
    test(`${period} checks respect restart time, repeated intervals and cancellation`, async t => {
        const start = 1_000_000;
        t.mock.timers.enable({apis: ["Date", "setTimeout"], now: start});
        let lastCheck = start;
        let checks = 0;
        const stop = scheduleUpdateChecks(period, () => lastCheck, async () => {
            checks++;
            lastCheck = Date.now();
        });
        // Advance daily to catch month-long browser timer overflows and premature requests.
        for (let day = 1; day <= 2 * days; day++) {
            t.mock.timers.tick(DAY_MS - 1);
            await Promise.resolve();
            assert.equal(checks, Math.floor((day - 1) / days));
            t.mock.timers.tick(1);
            await Promise.resolve();
            assert.equal(checks, Math.floor(day / days));
        }
        stop();
        t.mock.timers.tick(31 * DAY_MS);
        await Promise.resolve();
        assert.equal(checks, 2);
    });
}

test("first check waits five seconds; a manual check postpones the next automatic request", async t => {
    t.mock.timers.enable({apis: ["Date", "setTimeout"], now: 1_000_000});
    let lastCheck = 0;
    let checks = 0;
    const stop = scheduleUpdateChecks("day", () => lastCheck, async () => {
        checks++;
        lastCheck = Date.now();
    });
    t.mock.timers.tick(4999);
    assert.equal(checks, 0);
    t.mock.timers.tick(1);
    await Promise.resolve();
    assert.equal(checks, 1);
    t.mock.timers.tick(DAY_MS / 2);
    lastCheck = Date.now(); // The hook records manual checks before making the request as well.
    t.mock.timers.tick(DAY_MS / 2);
    await Promise.resolve();
    assert.equal(checks, 1);
    t.mock.timers.tick(DAY_MS / 2);
    await Promise.resolve();
    assert.equal(checks, 2);
    stop();
});
