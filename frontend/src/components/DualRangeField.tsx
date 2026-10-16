import {type CSSProperties, useEffect, useRef, useState} from "react";
import type {Interval} from "../tools/scatter";

export function DualRangeField({label, unit, value, min, max, step, onChange, minimumLabel, maximumLabel}: {
    label: string; unit: string; value: Interval; min: number; max: number; step: number;
    onChange: (value: Interval) => void; minimumLabel: string; maximumLabel: string;
}) {
    const [text, setText] = useState(value.map(String));
    const dragging = useRef<number | null>(null);
    useEffect(() => setText(value.map(String)), [value[0], value[1]]);
    const update = (index: number, next: number) => {
        const pair: Interval = [...value];
        pair[index] = Number(Math.max(index === 0 ? min : value[0], Math.min(index === 0 ? value[1] : max, next)).toFixed(6));
        setText(pair.map(String));
        onChange(pair);
    };
    const labels = [`${label}: ${minimumLabel}`, `${label}: ${maximumLabel}`];
    return <div className="range-field dual-range-field" role="group" aria-label={label}>
        <span>{label}{unit && <small>{unit}</small>}</span>
        <div className="dual-range-controls">
            {[0, 1].map(index => <input key={index} type="number" aria-label={labels[index]} value={text[index]}
                                        min={index === 0 ? min : value[0]} max={index === 0 ? value[1] : max}
                                        step={step}
                                        onChange={e => setText(previous => previous.map((v, n) => n === index ? e.target.value : v))}
                                        onBlur={() => {
                                            const next = Number(text[index]);
                                            if (text[index].trim() && Number.isFinite(next)) update(index, next);
                                            else setText(value.map(String));
                                        }} onKeyDown={e => {
                if (e.key === "Enter") e.currentTarget.blur();
            }}/>)}
            <div className="dual-range-track" onPointerDown={e => {
                if (e.button !== 0) return;
                e.preventDefault();
                const rect = e.currentTarget.getBoundingClientRect();
                const next = min + Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width)) * (max - min);
                const index = next <= (value[0] + value[1]) / 2 ? 0 : 1;
                dragging.current = index;
                e.currentTarget.setPointerCapture(e.pointerId);
                e.currentTarget.querySelectorAll("input")[index].focus();
                update(index, min + Math.round((next - min) / step) * step);
            }} onPointerMove={e => {
                if (dragging.current === null) return;
                const rect = e.currentTarget.getBoundingClientRect();
                const next = min + (e.clientX - rect.left) / rect.width * (max - min);
                update(dragging.current, min + Math.round((next - min) / step) * step);
            }} onPointerUp={() => {
                dragging.current = null;
            }} onPointerCancel={() => {
                dragging.current = null;
            }}
                 style={{
                     "--range-start": `${(value[0] - min) / (max - min) * 100}%`,
                     "--range-end": `${(value[1] - min) / (max - min) * 100}%`,
                 } as CSSProperties}>
                {[0, 1].map(index => <input key={index} type="range" aria-label={labels[index]}
                                            min={min} max={max} step={step} value={value[index]}
                                            aria-valuemin={index === 0 ? min : value[0]}
                                            aria-valuemax={index === 0 ? value[1] : max}
                                            style={{zIndex: index === 0 && value[0] >= max - step ? 3 : index + 1}}
                                            onChange={e => update(index, Number(e.target.value))}/>)}
            </div>
        </div>
    </div>;
}
