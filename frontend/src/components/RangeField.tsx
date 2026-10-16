import {useEffect, useRef, useState} from "react";

export function RangeField({label, unit, value, onChange, onInput, min = 0.25, max = 32, step = 0.25}: {
    label: string;
    unit: string;
    value: number;
    onChange: (value: number) => void;
    onInput?: (value: number) => void;
    min?: number;
    max?: number;
    step?: number;
}) {
    const [draft, setDraft] = useState(value);
    const [text, setText] = useState(String(value));
    const committed = useRef(value);
    const pendingInput = useRef(false);
    const lower = Math.min(min, value);
    const upper = Math.max(max, value);
    useEffect(() => {
        setDraft(value);
        setText(String(value));
        committed.current = value;
    }, [value]);
    const commit = (next: number) => {
        if (next !== committed.current || pendingInput.current) {
            pendingInput.current = false;
            committed.current = next;
            onChange(next);
        }
    };
    const commitText = () => {
        const next = Number(text);
        if (text.trim() !== "" && Number.isFinite(next) && next >= lower && next <= upper) {
            setDraft(next);
            setText(String(next));
            commit(next);
        } else {
            setText(String(draft));
        }
    };
    return <div className="range-field" role="group" aria-label={label}>
        <span>{label}</span>
        <div className="range-controls">
            <input type="range" aria-label={label} min={lower} max={upper} step={step}
                   value={draft} onChange={e => {
                const next = Number(e.target.value);
                setDraft(next);
                setText(String(next));
                if (onInput) {
                    pendingInput.current = true;
                    onInput(next);
                }
            }} onPointerUp={() => commit(draft)} onKeyUp={() => commit(draft)} onBlur={() => commit(draft)}/>
            <label className="range-value">
                <input type="number" aria-label={label} min={lower} max={upper} step={step}
                       value={text} onChange={e => setText(e.target.value)} onBlur={commitText}
                       onKeyDown={e => {
                           if (e.key === "Enter") e.currentTarget.blur();
                       }}/>
                {unit && <span>{unit}</span>}
            </label>
        </div>
    </div>;
}
