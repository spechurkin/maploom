import {useEffect, useRef, useState} from "react";

// Native color pickers emit many input events while dragging. Keep the control
// immediate, but commit only the final value after a short pause or on blur.
export function ColorField({label, value, onChange, disabled = false}: {
    label: string;
    value: string;
    onChange: (value: string) => void;
    disabled?: boolean;
}) {
    const [draft, setDraft] = useState(value);
    const input = useRef<HTMLInputElement>(null);
    const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
    const pending = useRef<{ value: string; commit: (value: string) => void } | null>(null);
    const submitted = useRef(value);
    const flush = () => {
        if (timer.current) clearTimeout(timer.current);
        timer.current = null;
        if (pending.current !== null) {
            const next = pending.current;
            pending.current = null;
            submitted.current = next.value;
            next.commit(next.value);
        }
    };
    useEffect(() => {
        if (!timer.current && (document.activeElement !== input.current || value === submitted.current))
            setDraft(value);
    }, [value]);
    useEffect(() => () => {
        if (timer.current) clearTimeout(timer.current);
    }, []);
    return <label className="color-field">
        <span>{label}</span>
        <input ref={input} type="color" aria-label={label} disabled={disabled}
               data-pending-color={pending.current !== null ? "true" : undefined}
               value={draft} onBlur={flush} onChange={(e) => {
            const next = e.target.value;
            setDraft(next);
            pending.current = {value: next, commit: onChange};
            if (timer.current) clearTimeout(timer.current);
            timer.current = setTimeout(flush, 100);
        }}/>
        <code>{draft.toUpperCase()}</code>
    </label>;
}
