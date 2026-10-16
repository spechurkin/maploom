import {useEffect, useRef, useState} from "react";
import {portalPreview, type PortalPreviewRequest} from "../app/bridge";
import type {GpuScene, GpuView} from "./gpu";

export type PortalPreviewResult = { key: string; scope: string; scene: GpuScene };

export const portalPreviewScope = (request: PortalPreviewRequest | null) =>
    JSON.stringify(request && {...request, position: undefined});

export function PortalPlacement({request, view, onScene, t}: {
    request: PortalPreviewRequest | null;
    view: GpuView;
    onScene: (result: PortalPreviewResult | null) => void;
    t: (key: string) => string;
}) {
    const key = JSON.stringify(request);
    const scope = portalPreviewScope(request);
    const pending = useRef<{ key: string; request: PortalPreviewRequest } | null>(null);
    const current = useRef<{ key: string; scope: string } | null>(null), running = useRef(false);
    const session = useRef({scope: "", epoch: 0});
    const [failure, setFailure] = useState<{ key: string; message: string } | null>(null);
    useEffect(() => {
        if (!request || session.current.scope !== scope) {
            session.current = {scope, epoch: session.current.epoch + 1};
            onScene(null);
        }
        current.current = request ? {key, scope} : null;
        pending.current = request ? {key, request} : null;
        // Coalesce mouse movement while a scene is building. Only one request
        // runs at a time. Completed frames remain visible during movement;
        // a changed asset, document or placement session rejects old results.
        const pump = async () => {
            if (running.current) return;
            running.current = true;
            try {
                while (pending.current) {
                    const job = pending.current;
                    const jobScope = current.current!.scope, epoch = session.current.epoch;
                    pending.current = null;
                    try {
                        const scene = await portalPreview(job.request);
                        if (current.current?.scope === jobScope && session.current.epoch === epoch) {
                            setFailure(null);
                            onScene({key: job.key, scope: jobScope, scene});
                        }
                    } catch (error) {
                        if (current.current?.key === job.key && session.current.epoch === epoch) {
                            onScene(null);
                            setFailure({key: job.key, message: String(error)});
                        }
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

    if (!request || failure?.key !== key) return null;
    return <div className="portal-placement-invalid" role="status" style={{
        left: view.origin.x + request.position.x * view.scale,
        top: view.origin.y + request.position.y * view.scale,
    }}><span aria-hidden="true">×</span><span>{t(failure.message)}</span></div>;
}
