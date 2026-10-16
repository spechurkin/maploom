export const UPDATE_PERIODS = ["day", "week", "month"] as const;
export type UpdatePeriod = typeof UPDATE_PERIODS[number];
export const DAY_MS = 24 * 60 * 60 * 1000;
const PERIOD_MS: Record<UpdatePeriod, number> = {day: DAY_MS, week: 7 * DAY_MS, month: 30 * DAY_MS};
const PERIOD_KEY = "maploom:update-period";
const LAST_CHECK_KEY = "maploom:update-last-check";

export function updatePeriod(value: unknown): UpdatePeriod {
    return UPDATE_PERIODS.includes(value as UpdatePeriod) ? value as UpdatePeriod : "day";
}

export function readUpdatePeriod(): UpdatePeriod {
    try {
        return updatePeriod(localStorage.getItem(PERIOD_KEY));
    } catch {
        return "day";
    }
}

export function readLastUpdateCheck(): number {
    try {
        const value = Number(localStorage.getItem(LAST_CHECK_KEY));
        return Number.isFinite(value) && value > 0 ? value : 0;
    } catch {
        return 0;
    }
}

export function saveUpdatePeriod(period: UpdatePeriod) {
    try {
        localStorage.setItem(PERIOD_KEY, period);
    } catch { /* Keep the selected period for this session if storage is unavailable. */ }
}

export function saveLastUpdateCheck(time: number) {
    try {
        localStorage.setItem(LAST_CHECK_KEY, String(time));
    } catch { /* The in-memory timestamp still prevents repeated checks this session. */ }
}

export function updateCheckDelay(lastCheck: number, period: UpdatePeriod, now = Date.now()): number {
    if (!lastCheck) return 0;
    const interval = PERIOD_MS[period];
    return Math.max(0, Math.min(interval, lastCheck + interval - now));
}

export function scheduleUpdateChecks(period: UpdatePeriod, lastCheck: () => number, check: () => Promise<void>) {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const schedule = (startup = false) => {
        // A month exceeds the maximum browser timer delay. Wake at least daily and recheck the clock.
        const delay = Math.max(startup ? 5000 : 1000, Math.min(DAY_MS, updateCheckDelay(lastCheck(), period)));
        timer = setTimeout(async () => {
            if (updateCheckDelay(lastCheck(), period) === 0) await check();
            if (!stopped) schedule();
        }, delay);
    };
    schedule(true);
    return () => {
        stopped = true;
        clearTimeout(timer);
    };
}
