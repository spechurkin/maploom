import type {Item} from "../map/model";
import type {GpuMesh, TextureUpload} from "./gpu";
import {textSize} from "../tools/text";

/** Browser-only preview; the desktop scene and exports use the shared Rust layout. */
export function previewText(item: Item, ppg: number, revision: number, ambient: number[], opacity: number): {
    mesh: GpuMesh;
    texture: TextureUpload
} | null {
    if (item.kind.type !== "text" || typeof document === "undefined") return null;
    const k = item.kind, style = item.text_style;
    const size = textSize(item, ppg);
    const canvas = document.createElement("canvas");
    const density = Math.min(ppg * 2, 4096 / Math.max(size.x, size.y, 1));
    canvas.width = Math.max(1, Math.ceil(size.x * density));
    canvas.height = Math.max(1, Math.ceil(size.y * density));
    const ctx = canvas.getContext("2d");
    if (!ctx) return null;
    ctx.scale(density / ppg, density / ppg);
    const color = (c: number[]) => `rgba(${c[0]},${c[1]},${c[2]},${c[3] / 255})`;
    const width = size.x * ppg, height = size.y * ppg;
    if (style?.background) {
        const border = style.border_width;
        ctx.beginPath();
        ctx.roundRect(border / 2, border / 2, Math.max(0, width - border), Math.max(0, height - border), style.shape === "rounded" ? Math.min(6, height / 2) : 0);
        ctx.fillStyle = color(style.background_color);
        ctx.fill();
        if (border > 0) {
            ctx.strokeStyle = color(style.border_color);
            ctx.lineWidth = border;
            ctx.stroke();
        }
    }
    ctx.fillStyle = color(k.color);
    ctx.font = `${k.size * ppg}px ${style?.font ? JSON.stringify(style.font) : "sans-serif"}`;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    const inset = style?.background ? 4 + style.border_width : 0;
    k.text.split("\n").forEach((line, n) => ctx.fillText(line, inset, inset + k.size * ppg * 1.2 * (n + 0.5)));
    const key = `preview:text:${item.id}:${revision}`;
    const vertices = [[0, 0], [1, 0], [1, 1], [0, 1]].flatMap(([x, y]) => [
        k.position.x + (x - 0.5) * size.x, k.position.y + (y - 0.5) * size.y, x, y,
        ambient[0] / 255, ambient[1] / 255, ambient[2] / 255, opacity, 0, 0, 0, 0,
    ]);
    const encode = (bytes: Uint8Array) => btoa(Array.from(bytes, b => String.fromCharCode(b)).join(""));
    return {
        mesh: {
            key,
            item: item.id,
            texture: key,
            mode: 0,
            clip: [0, 0, 512, 512],
            vertices: encode(new Uint8Array(new Float32Array(vertices).buffer)),
            indices: encode(new Uint8Array(new Uint32Array([0, 1, 2, 0, 2, 3]).buffer)),
            vertex_count: 4,
            index_count: 6
        },
        texture: {
            key,
            width: canvas.width,
            height: canvas.height,
            data: canvas.toDataURL("image/png").split(",")[1],
            format: "png",
            repeat: false,
            premultiplied: false,
            alias: null
        },
    };
}
