type Job = {
    key: string;
    load: () => Promise<string>;
    promise: Promise<string>;
    resolve: (src: string) => void;
    reject: (error: unknown) => void;
    users: number;
    state: "queued" | "running" | "settled";
};

/** Share requests, but never let unmounted cards keep a stale page in the queue. */
export class ThumbnailQueue {
    private cache = new Map<string, Job>();
    private pending: Job[] = [];
    private active = 0;

    constructor(private concurrency = 4, private capacity = 240) {
    }

    request(key: string, load: () => Promise<string>) {
        let job = this.cache.get(key);
        if (!job) {
            let resolve!: Job["resolve"], reject!: Job["reject"];
            const promise = new Promise<string>((yes, no) => {
                resolve = yes;
                reject = no;
            });
            job = {key, load, promise, resolve, reject, users: 0, state: "queued"};
            this.cache.set(key, job);
            this.pending.push(job);
        }
        // Recently requested entries survive cache trimming.
        this.cache.delete(key);
        this.cache.set(key, job);
        job.users++;
        this.pump();
        let released = false;
        return {
            promise: job.promise, release: () => {
                if (released) return;
                released = true;
                job.users--;
                if (!job.users && job.state === "queued") {
                    this.pending = this.pending.filter(entry => entry !== job);
                    if (this.cache.get(key) === job) this.cache.delete(key);
                    job.resolve("");
                }
            }
        };
    }

    reset() {
        // Running IPC calls finish normally; only the new generation is cached.
        for (const job of this.pending) job.resolve("");
        this.pending = [];
        this.cache.clear();
    }

    private pump() {
        while (this.active < this.concurrency && this.pending.length) {
            const job = this.pending.shift()!;
            job.state = "running";
            this.active++;
            void Promise.resolve().then(job.load).then(job.resolve, error => {
                // A temporary read failure must be retryable on remount.
                if (this.cache.get(job.key) === job) this.cache.delete(job.key);
                job.reject(error);
            }).finally(() => {
                job.state = "settled";
                this.active--;
                for (const [key, entry] of this.cache) {
                    if (this.cache.size <= this.capacity) break;
                    if (entry.state === "settled" && !entry.users) this.cache.delete(key);
                }
                this.pump();
            });
        }
    }
}
