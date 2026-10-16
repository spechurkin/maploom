import {useState} from "react";
import {ArrowDown, ArrowUp, ChevronLeft, ChevronRight} from "lucide-react";
import {type MapEdges, resizedMapSize, validMapSize} from "../map/mapSize";

export function MapResize({width, height, t, busy, onApply, onCancel}: {
    width: number; height: number; t: (key: string) => string; busy: boolean;
    onApply: (edges: MapEdges) => void; onCancel: () => void;
}) {
    const [draft, setDraft] = useState({top: "0", right: "0", bottom: "0", left: "0"});
    const edges = Object.fromEntries(Object.entries(draft).map(([k, v]) => [k, Number(v)])) as MapEdges;
    const size = resizedMapSize({width, height}, edges);
    const valid = Object.values(draft).every(v => v.trim() !== "" && Number.isInteger(Number(v)))
        && validMapSize(size.width, size.height);
    const changed = Object.values(edges).some(n => n !== 0);
    return <form onSubmit={e => {
        e.preventDefault();
        if (valid && changed && !busy) onApply(edges);
    }}>
        <div className="modal-body">
            <p className="muted">{t("Resize map hint")}</p>
            <div className="resize-compass">
                {(["top", "right", "bottom", "left"] as const).map(side => <label key={side}
                                                                                  className={`field resize-${side}`}>
                    <span>{t(`Map edge ${side}`)}</span>
                    <input type="number" step={1} required value={draft[side]}
                           onChange={e => setDraft({...draft, [side]: e.target.value})}/>
                </label>)}
                <div className="resize-center" aria-hidden="true">
                    <ArrowUp/><ChevronLeft/><span/><ChevronRight/><ArrowDown/></div>
            </div>
            <div className="resize-result" aria-live="polite"><span>{width} × {height}</span><ChevronRight size={18}/>
                <strong>{valid ? `${size.width} × ${size.height} ${t("cells")}` : t("Invalid size")}</strong></div>
            {!valid && <p role="alert">{t("Map dimensions must be 1–512 grid cells")}</p>}
            <p className="muted tiny">{t("Resize content hint")}</p>
        </div>
        <div className="modal-footer">
            <button type="button" className="button subtle" onClick={onCancel}>{t("Cancel")}</button>
            <button type="submit" className="button primary" disabled={busy || !valid || !changed}>{t("Apply")}</button>
        </div>
    </form>;
}
