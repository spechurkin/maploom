import {ArrowDown, ArrowUp, House, Layers, Plus, Trash2} from "lucide-react";
import type {Project} from "../map/model";
import {addLevel, mainLevelIndex, moveLevel, removeLevel} from "../map/levels";

export function FloorsPanel({project, level, busy, t, edit, select}: {
    project: Project; level: number; busy: boolean; t: (key: string) => string;
    edit: (change: (project: Project) => void) => Promise<boolean>;
    select: (id: string) => void;
}) {
    const main = mainLevelIndex(project), active = project.levels[level];
    const button = (label: string, icon: React.ReactNode, disabled: boolean, action: () => void) =>
        <button type="button" className="icon-button" title={t(label)} aria-label={t(label)}
                disabled={busy || disabled} onClick={action}>{icon}</button>;
    const add = (below: boolean) => {
        let id: string | null = null;
        void edit(p => {
            id = addLevel(p, `${t("Level")} ${p.levels.length + 1}`,
                p.levels.findIndex(l => l.id === active.id), below);
        })
            .then(ok => {
                if (ok && id) select(id);
            });
    };
    const move = (direction: -1 | 1) => void edit(p => {
        moveLevel(p, active.id, direction);
    });
    return <section className="floors-panel" aria-label={t("Levels")}>
        <div className="section-heading"><span>{t("Levels")}<small>{project.levels.length}</small></span>
            <div className="floor-add-actions">
                {button("Add floor above", <><Plus size={13}/><ArrowUp
                    size={12}/></>, project.levels.length >= 256, () => add(false))}
                {button("Add floor below", <><Plus size={13}/><ArrowDown
                    size={12}/></>, project.levels.length >= 256, () => add(true))}
            </div>
        </div>
        <div className="layers-list floors-list">
            {[...project.levels].reverse().map((floor, n) => {
                const index = project.levels.length - n - 1, isMain = index === main;
                return <div key={floor.id} className={`layer-row floor-row ${index === level ? "selected" : ""}`}>
                    <button type="button" className="layer-select" aria-pressed={index === level}
                            disabled={busy} onClick={() => select(floor.id)}>
                        {isMain ? <House size={16}/> : <Layers size={16}/>}
                        <span>{t(floor.name)}<small>{isMain ? t("Main floor") : `${t("Level")} ${index - main > 0 ? "+" : ""}${index - main}`}</small></span>
                    </button>
                    <span
                        className="level-badge">{isMain ? "0" : `${index - main > 0 ? "+" : ""}${index - main}`}</span>
                </div>;
            })}
        </div>
        <div className="layer-settings">
            <label className="field"><span>{t("Floor name")}</span>
                <input key={`${active.id}:${active.name}`} aria-label={t("Floor name")} defaultValue={active.name}
                       disabled={busy} onBlur={e => {
                    const name = e.target.value.trim();
                    if (name && name !== active.name) void edit(p => {
                        p.levels.find(l => l.id === active.id)!.name = name;
                    });
                    else e.target.value = active.name;
                }} onKeyDown={e => {
                    if (e.key === "Enter") e.currentTarget.blur();
                }}/>
            </label>
            <div className="layer-actions">
                {button("Move floor up", <ArrowUp
                    size={15}/>, level === main || level + 1 === main || level === project.levels.length - 1, () => move(1))}
                {button("Move floor down", <ArrowDown
                    size={15}/>, level === main || level - 1 === main || level === 0, () => move(-1))}
                {button("Delete level", <Trash2
                    size={15}/>, level === main || active.layers.some(l => l.locked || l.items.some(i => i.locked)), () => {
                    void edit(p => {
                        removeLevel(p, active.id);
                    });
                })}
            </div>
            {level === main && <p className="muted tiny">{t("Main floor fixed")}</p>}
        </div>
    </section>;
}
