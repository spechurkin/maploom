import {useEffect, useRef} from "react";
import {drawingPreview, type DrawingPreviewRequest} from "../app/bridge";
import type {GpuScene} from "./gpu";

export type DrawingPreviewResult = { scope: string; scene: GpuScene };

export function DrawingPreview({request, scope, onScene}: {
    request: DrawingPreviewRequest | null;
    scope: string;
    onScene: (result: DrawingPreviewResult | null) => void;
}) {
    const pending = useRef<DrawingPreviewRequest | null>(null);
    const current = useRef<{ scope: string; request: DrawingPreviewRequest } | null>(null);
    const running = useRef(false);
    const session = useRef({scope: "", epoch: 0});
    const key = JSON.stringify(request);
    useEffect(() => {
        if (!request || session.current.scope !== scope) session.current = {scope, epoch: session.current.epoch + 1};
        current.current = request ? {scope, request} : null;
        pending.current = request;
        const pump = async () => {
            if (running.current) return;
            running.current = true;
            try {
                while (pending.current) {
                    const job = pending.current, jobScope = current.current!.scope, epoch = session.current.epoch;
                    pending.current = null;
                    try {
                        const scene = await drawingPreview(job);
                        // Show completed frames during continuous movement, but never
                        // resurrect a cancelled gesture or a previous tool/document.
                        if (current.current?.scope === jobScope && session.current.epoch === epoch) onScene({
                            scope: jobScope,
                            scene
                        });
                    } catch {
                        if (current.current?.request === job) onScene(null);
                    }
                }
            } finally {
                running.current = false;
            }
        };
        void pump();
        return () => {
            current.current = null;
            pending.current = null;
        };
    }, [key, scope, onScene]);
    return null;
}
