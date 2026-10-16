import {type CSSProperties, type RefObject, useEffect, useRef} from "react";
import {GpuRenderer, type GpuScene, type GpuView, type Overlay, retainGpuResources,} from "./gpu";

export function GpuCanvas({
                              scene,
                              retainedScene,
                              view,
                              overlay,
                              onReady,
                              onError,
                              onRecovery,
                              picker,
                              className = "map-render",
                              style,
                          }: {
    scene: GpuScene | null;
    retainedScene?: GpuScene | null;
    view: GpuView;
    overlay?: Overlay;
    onReady?: (revision: number) => void;
    onError?: (message: string) => void;
    onRecovery?: () => void;
    picker?: RefObject<GpuRenderer | null>;
    className?: string;
    style?: CSSProperties;
}) {
    const canvas = useRef<HTMLCanvasElement>(null),
        renderer = useRef<GpuRenderer | null>(null),
        latest = useRef({view, overlay, onReady, onError, onRecovery});
    latest.current = {view, overlay, onReady, onError, onRecovery};
    useEffect(() => {
        const element = canvas.current!;
        const init = () => {
            try {
                renderer.current = new GpuRenderer(element);
                if (picker) picker.current = renderer.current;
            } catch (e) {
                latest.current.onError?.(String(e instanceof Error ? e.message : e));
            }
        };
        const lost = (e: Event) => {
            e.preventDefault();
            renderer.current?.dispose();
            renderer.current = null;
            if (picker) picker.current = null;
            latest.current.onError?.("GPU context lost");
        };
        const restore = () => {
            renderer.current?.dispose();
            init();
            latest.current.onRecovery?.();
        };
        init();
        element.addEventListener("webglcontextlost", lost);
        element.addEventListener("webglcontextrestored", restore);
        return () => {
            renderer.current?.dispose();
            renderer.current = null;
            if (picker) picker.current = null;
            element.removeEventListener("webglcontextlost", lost);
            element.removeEventListener("webglcontextrestored", restore);
        };
    }, []);
    useEffect(() => {
        if (!scene || !renderer.current) return;
        let alive = true;
        const active = renderer.current;
        if (picker) picker.current = null;
        // A transient placement scene must not evict the original wall meshes
        // needed when the cursor leaves the map or the tool changes.
        retainGpuResources([...scene.used_textures, ...(retainedScene?.used_textures ?? [])],
            [...scene.meshes, ...(retainedScene?.meshes ?? [])].map((m) => m.key));
        active.setScene(scene).then(
            (ok) => {
                if (alive && ok && renderer.current === active) {
                    if (picker) picker.current = active;
                    active.draw(latest.current.view, latest.current.overlay);
                    latest.current.onReady?.(scene.revision);
                }
            },
            (e) => {
                if (alive && renderer.current === active)
                    latest.current.onError?.(String(e instanceof Error ? e.message : e));
            },
        );
        return () => {
            alive = false;
        };
    }, [scene, retainedScene]);
    useEffect(() => {
        const frame = requestAnimationFrame(() =>
            renderer.current?.draw(view, overlay),
        );
        return () => cancelAnimationFrame(frame);
    }, [
        view.origin.x,
        view.origin.y,
        view.scale,
        view.width,
        view.height,
        overlay,
    ]);
    return (
        <canvas
            ref={canvas}
            className={className}
            style={style}
            aria-label="Map canvas"
        />
    );
}
