import type {Item, Point, Project} from "../map/model";

export type TextBounds = { x: number; y: number; right: number; bottom: number };
const measured = new Map<string, { signature: string; size: Point }>();
const signature = (i: Item) => JSON.stringify([i.kind.type === "text" ? [i.kind.text, i.kind.size] : null, i.text_style]);

export function rememberTextBounds(project: Project, bounds: Record<string, TextBounds>) {
    measured.clear();
    for (const level of project.levels) for (const layer of level.layers) for (const i of layer.items) {
        const b = bounds[i.id];
        if (i.kind.type === "text" && b)
            measured.set(i.id, {signature: signature(i), size: {x: b.right - b.x, y: b.bottom - b.y}});
    }
}

export function textSize(i: Item, ppg = 100): Point {
    if (i.kind.type !== "text") return {x: 0, y: 0};
    const cached = measured.get(i.id);
    if (cached?.signature === signature(i)) return cached.size;
    // Browser preview / first frame fallback. Desktop uses exact renderer metrics above.
    const lines = i.kind.text.split("\n"), pixels = i.kind.size * ppg;
    const ctx = typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
    if (ctx) ctx.font = `${pixels}px ${i.text_style?.font ? JSON.stringify(i.text_style.font) : "sans-serif"}`;
    const width = Math.max(0, ...lines.map(line => ctx ? ctx.measureText(line).width : [...line].length * pixels * 0.6));
    const margin = i.text_style?.background ? (4 + i.text_style.border_width) / ppg : 0;
    return {x: width / ppg + 2 * margin, y: Math.max(1, lines.length) * i.kind.size * 1.2 + 2 * margin};
}
