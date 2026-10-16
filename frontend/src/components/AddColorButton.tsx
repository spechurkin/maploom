import {useEffect, useRef} from "react";
import {Plus} from "lucide-react";

export function AddColorButton({label, value, onAdd}: {
    label: string;
    value: string;
    onAdd: (color: string) => void;
}) {
    const input = useRef<HTMLInputElement>(null);
    useEffect(() => {
        const picker = input.current;
        if (!picker) return;
        // React's onChange also handles intermediate input events. Only the
        // native change event commits the final selection from the picker.
        const commit = () => onAdd(picker.value);
        picker.addEventListener("change", commit);
        return () => picker.removeEventListener("change", commit);
    }, [onAdd]);

    return <span className="add-color">
        <button type="button" className="button subtle compact" aria-label={label} title={label}
                onClick={() => {
                    const picker = input.current;
                    if (!picker) return;
                    picker.value = value;
                    if (typeof picker.showPicker === "function") picker.showPicker();
                    else picker.click();
                }}><Plus size={14}/></button>
        <input ref={input} type="color" tabIndex={-1} aria-hidden="true"/>
    </span>;
}
