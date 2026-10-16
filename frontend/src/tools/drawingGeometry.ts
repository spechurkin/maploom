import {circleContour, type DrawShape, rectangle, samePoint} from "./architecture";
import type {Point, Tool} from "../map/model";
import {simplifyPath} from "./paths";

/** The tentative endpoint participates in the same geometry as the final click. */
export function drawingPoints(tool: Tool, shape: DrawShape, gesture: Point[], cursor: Point | null,
                              drag: { mode: string; start: Point; last: Point; trail: Point[] } | null,
                              segmentation: number, scale: number): Point[] {
    if (tool === "path" && drag?.mode === "path") return simplifyPath(drag.trail, 1.5 / scale);
    if (drag?.mode === "room") {
        const points = shape === "circle" ? circleContour(drag.start, drag.last, segmentation)
            : rectangle(drag.start, drag.last);
        return tool === "wall" ? [...points, {...points[0]}] : points;
    }
    if (drag || !gesture.length) return [];
    const points = gesture.filter((p, n) => !n || !samePoint(p, gesture[n - 1]));
    if ((tool === "room" || tool === "floor") && points.length >= 3 && cursor && samePoint(cursor, points[0])) return points;
    return cursor && !samePoint(cursor, points.at(-1)!) ? [...points, cursor] : points;
}
