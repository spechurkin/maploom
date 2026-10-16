import {Minus, Plus} from "lucide-react";
import type {Project} from "../map/model";
import type {Action} from "../app/bridge";
import {variationOptions} from "../map/variations";

export function VariationsPanel({project, busy, t, run}: {
    project: Project; busy: boolean; t: (key: string) => string;
    run: (action: Action) => Promise<boolean>;
}) {
    const options = variationOptions(project);
    const active = options.find(v => v.id === project.active_variation) ?? options[0];
    return <section className="variations-panel" aria-label={t("Variations")}>
        <div className="section-heading"><span>{t("Variations")}<small>{options.length}</small></span>
            <div className="floor-add-actions">
                <button type="button" className="icon-button" title={t("Add variation")} aria-label={t("Add variation")}
                        disabled={busy} onClick={() => {
                    let number = options.length + 1;
                    while (options.some(v => v.name === `${t("Variation")} ${number}`)) number++;
                    void run({type: "add_variation", name: `${t("Variation")} ${number}`});
                }}><Plus size={15}/></button>
                <button type="button" className="icon-button" title={t("Delete variation")}
                        aria-label={t("Delete variation")}
                        disabled={busy || options.length < 2} onClick={() => void run({type: "remove_variation"})}>
                    <Minus size={15}/></button>
            </div>
        </div>
        <label className="field"><span>{t("Selected variation")}</span>
            <select aria-label={t("Selected variation")} value={active.id} disabled={busy}
                    onChange={e => void run({type: "select_variation", id: e.target.value})}>
                {options.map(v => <option key={v.id} value={v.id}>{t(v.name)}</option>)}
            </select>
        </label>
        <label className="field"><span>{t("Variation name")}</span>
            <input key={`${active.id}:${active.name}`} aria-label={t("Variation name")} defaultValue={t(active.name)}
                   disabled={busy} onBlur={e => {
                const name = e.target.value.trim();
                if (name && name !== t(active.name)) void run({type: "rename_variation", name});
                else e.target.value = t(active.name);
            }} onKeyDown={e => {
                if (e.key === "Enter") e.currentTarget.blur();
            }}/>
        </label>
        <p className="muted tiny">{t("Variation copy hint")}</p>
    </section>;
}
