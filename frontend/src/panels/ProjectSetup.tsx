import {useState} from "react";
import {Plus, Sparkles, Trash2} from "lucide-react";
import {type MapSizePreset, uid} from "../map/model";
import {builtInMapSizes, validMapSize} from "../map/mapSize";

export function ProjectSetup({t, presets, busy, onPresets, onCreate, onDemo, onCancel}: {
    t: (key: string) => string; presets: MapSizePreset[]; busy: boolean;
    onPresets: (presets: MapSizePreset[]) => Promise<boolean>;
    onCreate: (name: string, width: number, height: number) => void;
    onDemo: () => void; onCancel: () => void;
}) {
    const [name, setName] = useState("");
    const [width, setWidth] = useState("32"), [height, setHeight] = useState("24");
    const [presetId, setPresetId] = useState("default"), [presetName, setPresetName] = useState("");
    const w = Number(width), h = Number(height), valid = validMapSize(w, h);
    const select = (id: string) => {
        setPresetId(id);
        const preset = [...builtInMapSizes, ...presets].find(p => p.id === id);
        if (preset) {
            setWidth(String(preset.width));
            setHeight(String(preset.height));
        }
    };
    return <form onSubmit={e => {
        e.preventDefault();
        if (valid && !busy) onCreate(name.trim() || t("Untitled"), w, h);
    }}>
        <div className="modal-body">
            <label className="field"><span>{t("Map name")}</span>
                <input autoFocus value={name} onChange={e => setName(e.target.value)}
                       placeholder={t("Untitled")}/></label>
            <div className="preset-select">
                <label className="field"><span>{t("Size template")}</span>
                    <select aria-label={t("Size template")} value={presetId} onChange={e => select(e.target.value)}>
                        <option value="">{t("Custom size")}</option>
                        <optgroup label={t("Built-in presets")}>{builtInMapSizes.map(p =>
                            <option key={p.id}
                                    value={p.id}>{t(p.name)}{p.name === `${p.width} × ${p.height}` ? "" : ` · ${p.width} × ${p.height}`}</option>)}</optgroup>
                        {presets.length > 0 && <optgroup label={t("My presets")}>{presets.map(p =>
                            <option key={p.id} value={p.id}>{p.name} · {p.width} × {p.height}</option>)}</optgroup>}
                    </select>
                </label>
                <button type="button" className="icon-button" title={t("Delete preset")} aria-label={t("Delete preset")}
                        disabled={busy || !presets.some(p => p.id === presetId)} onClick={async () => {
                    if (await onPresets(presets.filter(p => p.id !== presetId))) setPresetId("");
                }}><Trash2 size={17}/></button>
            </div>
            <div
                className="two-fields">{([["Map width", width, setWidth], ["Height", height, setHeight]] as const).map(([label, value, set]) =>
                <label className="field" key={label}><span>{t(label)}</span>
                    <input type="number" min={1} max={512} step={1} required value={value}
                           onChange={e => {
                               set(e.target.value);
                               setPresetId("");
                           }}/></label>)}</div>
            <p className="muted tiny">{valid ? `${w} × ${h} ${t("cells")}` : t("Map dimensions must be 1–512 grid cells")}</p>
            <div className="preset-save">
                <label className="field"><span>{t("Preset name")}</span><input value={presetName} maxLength={80}
                                                                               placeholder={t("My map size")}
                                                                               onChange={e => setPresetName(e.target.value)}/></label>
                <button type="button" className="button subtle"
                        disabled={busy || !valid || !presetName.trim() || presets.length >= 100}
                        onClick={async () => {
                            const preset = {id: uid(), name: presetName.trim(), width: w, height: h};
                            if (await onPresets([...presets, preset])) {
                                setPresetId(preset.id);
                                setPresetName("");
                            }
                        }}>{t("Save preset")}</button>
            </div>
            <button type="button" className="sample-card" disabled={busy} onClick={onDemo}><Sparkles
                size={20}/><span>{t("Demo")}</span></button>
        </div>
        <div className="modal-footer">
            <button type="button" className="button subtle" onClick={onCancel}>{t("Cancel")}</button>
            <button type="submit" className="button primary" disabled={busy || !valid}><Plus size={16}/>{t("Create")}
            </button>
        </div>
    </form>;
}
