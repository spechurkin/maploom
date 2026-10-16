import {type Project, uid, type Variation} from "./model";

export const variationOptions = (p: Project) => p.variations?.length
    ? p.variations.map(({id, name}) => ({id, name})) : [{id: p.id, name: "Variation 1"}];

const sceneContent = (p: Project): Omit<Variation, "id" | "name"> => structuredClone({
    levels: p.levels, main_level: p.main_level, background: p.background,
    ambient_light: p.ambient_light ?? [255, 255, 255, 255], background_asset: p.background_asset,
});

export function syncActiveVariation(p: Project) {
    const active = p.variations?.find(v => v.id === p.active_variation);
    if (active) Object.assign(active, sceneContent(p));
}

function initialize(p: Project) {
    if (!p.variations?.length) {
        p.active_variation = p.id;
        p.variations = [{id: p.id, name: "Variation 1", ...sceneContent(p)}];
    }
}

export function selectVariation(p: Project, id: string) {
    if (!p.variations?.some(v => v.id === id)) throw Error("Invalid variation");
    syncActiveVariation(p);
    const {
        levels,
        main_level,
        background,
        ambient_light,
        background_asset
    } = structuredClone(p.variations.find(v => v.id === id)!);
    Object.assign(p, {levels, main_level, background, ambient_light, background_asset, active_variation: id});
}

export function addVariation(p: Project, name: string) {
    if (!name.trim()) throw Error("Variation name is required");
    initialize(p);
    syncActiveVariation(p);
    const copy = structuredClone(p.variations!.find(v => v.id === p.active_variation)!);
    copy.id = uid();
    copy.name = name;
    p.variations!.push(copy);
    p.active_variation = copy.id;
}

export function removeVariation(p: Project) {
    if (!p.variations || p.variations.length < 2) throw Error("The last variation cannot be deleted");
    const index = p.variations.findIndex(v => v.id === p.active_variation);
    if (index < 0) throw Error("Invalid variation");
    selectVariation(p, p.variations[index === 0 ? 1 : index - 1].id);
    p.variations.splice(index, 1);
}

export function renameVariation(p: Project, name: string) {
    if (!name.trim()) throw Error("Variation name is required");
    initialize(p);
    p.variations!.find(v => v.id === p.active_variation)!.name = name;
}
