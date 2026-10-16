import {newLevel, type Project} from "./model";

export function mainLevelIndex(project: Project): number {
    return Math.max(0, project.levels.findIndex(l => l.id === project.main_level));
}

export function protectMainLevel(previous: Project, next: Project) {
    const main = previous.levels[mainLevelIndex(previous)].id;
    const nextMain = next.levels.findIndex(l => l.id === main);
    if (nextMain < 0 || (next.main_level != null && next.main_level !== main) ||
        previous.levels.some((floor, index) => {
            const target = next.levels.findIndex(l => l.id === floor.id);
            return target >= 0 && Math.sign(index - mainLevelIndex(previous)) !== Math.sign(target - nextMain);
        })) throw Error("Main floor is fixed");
    if (previous.main_level || nextMain !== 0) next.main_level = main;
}

export function addLevel(project: Project, name: string, index: number, below = false): string | null {
    if (project.levels.length >= 256) return null;
    if (index < 0 || index >= project.levels.length) return null;
    project.main_level ??= project.levels[0].id;
    const level = newLevel(name);
    project.levels.splice(index + (below ? 0 : 1), 0, level);
    return level.id;
}

export function moveLevel(project: Project, id: string, direction: -1 | 1): boolean {
    const index = project.levels.findIndex(l => l.id === id), target = index + direction;
    if (index < 0 || target < 0 || target >= project.levels.length) return false;
    const main = mainLevelIndex(project);
    if (index === main || target === main) return false;
    project.main_level ??= project.levels[0].id;
    [project.levels[index], project.levels[target]] = [project.levels[target], project.levels[index]];
    return true;
}

export function removeLevel(project: Project, id: string): boolean {
    const index = project.levels.findIndex(l => l.id === id);
    if (index < 0 || index === mainLevelIndex(project) || project.levels.length === 1) return false;
    const level = project.levels[index];
    if (level.layers.some(l => l.locked || l.items.some(i => i.locked))) return false;
    project.main_level ??= project.levels[0].id;
    project.levels.splice(index, 1);
    return true;
}
