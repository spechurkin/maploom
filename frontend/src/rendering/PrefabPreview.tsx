import type {AssetLookup, Item, Point} from "../map/model";
import {placePrefab, type Prefab, prefabBounds} from "../map/prefabs";
import {Thumbnail} from "../components/Thumbnail";
import {ObjectPlacement} from "./ObjectPlacement";
import type {GpuView} from "./gpu";

export function PrefabPlacement({prefab, assets, position, scale, rotation, view}: {
    prefab: Prefab; assets: AssetLookup; position: Point; scale: number; rotation: number; view: GpuView;
}) {
    const preview = prefab.preview ? assets[prefab.preview] : undefined;
    return preview ? <ObjectPlacement asset={preview} position={position} scale={scale} rotation={rotation}
                                      color={[255, 255, 255, 255]} view={view}/> :
        <div className="prefab-placement" aria-hidden="true">
            <PrefabSprites items={placePrefab(prefab, position, scale, rotation * Math.PI / 180, false)}
                           assets={assets} origin={view.origin} scale={view.scale}/></div>;
}

export function PrefabSprites({items, assets, origin, scale, maxSide = 1024}: {
    items: Item[]; assets: AssetLookup; origin: Point; scale: number; maxSide?: number;
}) {
    return <>{items.map((item, index) => {
        const k = item.kind;
        if (k.type !== "object") return null;
        const meta = assets[k.asset];
        const colorable = !!meta && (meta.color_mode !== "none" || meta.tags.some(tag => tag.toLowerCase() === "colorable"));
        return <div key={index} className="prefab-sprite" style={{
            left: origin.x + k.position.x * scale, top: origin.y + k.position.y * scale,
            width: k.size.x * scale, height: k.size.y * scale,
            transform: `rotate(${k.rotation}rad) translate(${-k.pivot.x * 100}%, ${-k.pivot.y * 100}%)`,
            opacity: item.tint[3] / 255,
        }}>
            <div
                style={{width: "100%", height: "100%", transform: `scale(${k.flip_x ? -1 : 1}, ${k.flip_y ? -1 : 1})`}}>
                <Thumbnail id={k.asset} color={colorable ? [item.tint[0], item.tint[1], item.tint[2], 255] : undefined}
                           maxSide={maxSide}/>
            </div>
        </div>;
    })}</>;
}

export function PrefabThumbnail({prefab, assets}: { prefab: Prefab; assets: AssetLookup }) {
    if (prefab.preview) return <Thumbnail id={prefab.preview} maxSide={256}
                                          frameClassName="prefab-thumbnail prefab-baked-thumbnail"/>;
    const b = prefabBounds(prefab.items);
    const scale = 88 / Math.max(b.right - b.x, b.bottom - b.y, 0.001);
    return <svg className="prefab-thumbnail" viewBox="0 0 100 100" aria-hidden="true">
        <foreignObject width="100" height="100">
            <div className="prefab-thumbnail-canvas">
                <PrefabSprites items={prefab.items} assets={assets} scale={scale} maxSide={128}
                               origin={{x: 50 - (b.x + b.right) * scale / 2, y: 50 - (b.y + b.bottom) * scale / 2}}/>
            </div>
        </foreignObject>
    </svg>;
}
