import type {AssetMeta, Color, Kind, Point} from "../map/model";
import type {GpuView} from "./gpu";
import {Thumbnail} from "../components/Thumbnail";

export function objectPlacement(asset: AssetMeta, position: Point, scale: number, degrees: number): Extract<Kind, {
    type: "object"
}> {
    return {
        type: "object",
        asset: asset.id,
        position,
        size: {x: asset.size.x * scale, y: asset.size.y * scale},
        rotation: degrees * Math.PI / 180,
        pivot: asset.pivot,
        flip_x: false,
        flip_y: false,
    };
}

export function ObjectPlacement({asset, position, scale, rotation, color, view}: {
    asset: AssetMeta;
    position: Point | null;
    scale: number;
    rotation: number;
    color: Color;
    view: GpuView;
}) {
    const kind = objectPlacement(asset, position ?? {x: 0, y: 0}, scale, rotation);
    // Keep the image mounted while the tool is active so entering the map does
    // not start another asset request. Only the transform changes on movement.
    return <div className="object-placement" aria-hidden="true" style={{
        display: position ? undefined : "none",
        left: view.origin.x + kind.position.x * view.scale,
        top: view.origin.y + kind.position.y * view.scale,
    }}>
        <div className="object-placement-sprite" style={{
            width: kind.size.x * view.scale,
            height: kind.size.y * view.scale,
            transform: `rotate(${kind.rotation}rad) translate(${-kind.pivot.x * 100}%, ${-kind.pivot.y * 100}%)`,
        }}>
            <Thumbnail id={asset.id} color={asset.color_mode === "none" ? undefined : color} maxSide={1024}/>
        </div>
    </div>;
}
