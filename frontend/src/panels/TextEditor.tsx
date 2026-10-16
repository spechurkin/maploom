import {useEffect, useRef, useState} from "react";
import {Move, Pencil, Plus} from "lucide-react";
import {desktop, localFonts} from "../app/bridge";
import {type Color, defaultTextStyle, hex, type Kind, rgba, type TextStyle} from "../map/model";
import {ColorField} from "../components/ColorField";
import {RangeField} from "../components/RangeField";

export type TextMode = "edit" | "move";
export type TextKind = Extract<Kind, { type: "text" }>;

function AlphaColor({label, value, onChange, t}: {
    label: string;
    value: Color;
    onChange: (v: Color) => void;
    t: (s: string) => string
}) {
    return <>
        <ColorField label={t(label)} value={hex(value)}
                    onChange={v => onChange([...rgba(v).slice(0, 3), value[3]] as Color)}/>
        <RangeField label={t(label) + ": " + t("Opacity")} unit="%" min={0} max={100} step={1}
                    value={Math.round(value[3] / 255 * 100)}
                    onChange={v => onChange([value[0], value[1], value[2], Math.round(v / 100 * 255)])}/>
    </>;
}

export function TextEditor({
                               value,
                               style: suppliedStyle,
                               ppg,
                               mode,
                               onMode,
                               onChange,
                               onStyle,
                               onNew,
                               t,
                               focus = false,
                               live = false
                           }: {
    value: TextKind;
    style?: TextStyle | null;
    ppg: number;
    mode: TextMode;
    onMode: (v: TextMode) => void;
    onChange: (v: Partial<Pick<TextKind, "text" | "size" | "color">>) => void;
    onStyle: (v: TextStyle) => void;
    onNew?: () => void;
    t: (s: string) => string;
    focus?: boolean;
    live?: boolean;
}) {
    const style = {...defaultTextStyle(), ...suppliedStyle};
    const [fonts, setFonts] = useState<string[]>([]), [fontError, setFontError] = useState(false);
    const [draft, setDraft] = useState(value.text);
    const input = useRef<HTMLTextAreaElement>(null);
    const pending = useRef(value.text);
    const submitted = useRef(value.text);
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const commit = () => {
        if (timer.current) clearTimeout(timer.current);
        timer.current = null;
        if (pending.current !== submitted.current) {
            submitted.current = pending.current;
            onChange({text: pending.current});
        }
    };
    useEffect(() => {
        // Acknowledging an earlier save must not replace characters typed since it.
        if (value.text === submitted.current && pending.current !== value.text) return;
        setDraft(value.text);
        pending.current = value.text;
        submitted.current = value.text;
    }, [value.text]);
    useEffect(() => () => {
        if (timer.current) clearTimeout(timer.current);
    }, []);
    useEffect(() => {
        if (focus && mode === "edit") input.current?.focus();
    }, [focus, mode]);
    useEffect(() => {
        let active = true;
        localFonts().then(v => {
            if (active) setFonts(v);
        }, () => {
            if (active) setFontError(true);
        });
        return () => {
            active = false;
        };
    }, []);
    const missing = !!style.font && !fonts.includes(style.font);
    return <div className="text-editor">
        <div className="text-modes" role="group" aria-label={t("Text mode")}>
            <button type="button" className={mode === "edit" ? "active" : ""} aria-pressed={mode === "edit"}
                    onClick={() => {
                        commit();
                        onMode("edit");
                    }}><Pencil size={15}/>{t("Edit text")}</button>
            <button type="button" className={mode === "move" ? "active" : ""} aria-pressed={mode === "move"}
                    onClick={() => {
                        commit();
                        onMode("move");
                    }}><Move size={15}/>{t("Move text")}</button>
        </div>
        {onNew && <button type="button" className="text-new" onClick={() => {
            commit();
            onNew();
        }}><Plus size={15}/>{t("New text")}</button>}
        <p className="muted tiny">{t(mode === "move" ? "Text move hint" : "Text edit hint")}</p>
        <label className="field"><span>{t("Text")}</span>
            <textarea ref={input} aria-label={t("Text")} value={draft} readOnly={mode === "move"} rows={4}
                      onChange={e => {
                          pending.current = e.target.value;
                          setDraft(e.target.value);
                          if (live) commit();
                          else {
                              if (timer.current) clearTimeout(timer.current);
                              timer.current = setTimeout(commit, 200);
                          }
                      }}
                      onBlur={commit} onKeyDown={e => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                    commit();
                    e.currentTarget.blur();
                }
            }}/>
        </label>
        <label className="field"><span>{t("Font")}</span>
            <select aria-label={t("Font")} value={style.font} onChange={e => onStyle({...style, font: e.target.value})}>
                <option value="">{t("Bundled font")}</option>
                {missing && <option value={style.font}>{style.font} — {t("Font unavailable")}</option>}
                {fonts.map(name => <option key={name} value={name}>{name}</option>)}
            </select>
        </label>
        {(!desktop || fontError || missing) &&
            <p className="muted tiny">{t(!desktop ? "Local fonts desktop hint" : fontError ? "Local fonts error" : "Font fallback hint")}</p>}
        <RangeField label={t("Font size")} unit="px" value={Math.round(value.size * ppg * 100) / 100}
                    min={1} max={1000} step={1} onChange={v => onChange({size: v / ppg})}/>
        <AlphaColor label="Font color" value={value.color} onChange={color => onChange({color})} t={t}/>
        <label className="text-background-toggle"><span>{t("Text box")}</span>
            <input type="checkbox" checked={style.background}
                   onChange={e => onStyle({...style, background: e.target.checked})}/>
        </label>
        {style.background && <>
            <label className="field"><span>{t("Shape")}</span>
                <select aria-label={t("Text box shape")} value={style.shape}
                        onChange={e => onStyle({...style, shape: e.target.value as TextStyle["shape"]})}>
                    <option value="square">{t("Square")}</option>
                    <option value="rounded">{t("Rounded")}</option>
                </select>
            </label>
            <RangeField label={t("Border width")} unit="px" value={style.border_width} min={0} max={64} step={1}
                        onChange={border_width => onStyle({...style, border_width})}/>
            <AlphaColor label="Background color" value={style.background_color}
                        onChange={background_color => onStyle({...style, background_color})} t={t}/>
            <AlphaColor label="Border color" value={style.border_color}
                        onChange={border_color => onStyle({...style, border_color})} t={t}/>
            <p className="muted tiny">{t("Text padding hint")}</p>
        </>}
    </div>;
}
