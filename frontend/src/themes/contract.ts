// Public theme API v1. Keep this independent of the editor model and bridge.
export const themeTokens = ["--accent", "--accent-ink", "--bg", "--panel", "--panel-alt", "--input", "--text", "--muted", "--faint", "--line", "--hover", "--canvas", "--active", "--shadow", "--texture", "--theme-font"] as const;
export type ThemeMode = "dark" | "light";
export type ThemeBranding = { name: string; logo: string };
export type ThemeVariant = { id: string; name: string; mode: ThemeMode };
export type Appearance = Readonly<{
    mode: ThemeMode;
    accent: readonly number[];
    locale: string;
    screen: "welcome" | "editor"
}>;
export type Theme = {
    id: string;
    variant?: string | null;
    manifest: {
        format: "maploom/theme";
        api_version: 1;
        name: string;
        description: string;
        mode?: ThemeMode | null;
        variants?: ThemeVariant[];
        stylesheet: string | null;
        entry: string | null;
        branding?: ThemeBranding | null;
    }
};
export type ThemeCatalog = { directory: string; themes: Theme[]; warnings: string[] };
export type ThemePackage = { theme: Theme; modules: Record<string, string> };

/** Fixed theme types override the user's base mode; adaptive themes inherit it. */
export function resolveThemeMode(mode: ThemeMode | null | undefined, fallback: ThemeMode): ThemeMode {
    return mode ?? fallback;
}

export function themeKey(theme: Theme): string {
    return JSON.stringify([theme.id, theme.variant ?? null]);
}

export function isThemeSelected(theme: Theme, id: string | null | undefined, variant: string | null | undefined): boolean {
    return theme.id === id && (theme.variant ?? null) === (variant ?? theme.manifest.variants?.[0]?.id ?? null);
}

export interface ThemeAPI {
    readonly version: 1;
    readonly root: HTMLElement;
    readonly appearance: Appearance;

    asset(path: string): string;

    setTokens(tokens: Record<string, string>): void;

    onAppearanceChange(listener: (appearance: Appearance) => void): () => void;
}

export function assetURL(base: string, path: string) {
    if (!path || path.split("/").some(p => !p || p === "." || p === "..") || /[\\:\0?#]/.test(path)) throw Error("Expected a relative theme asset path");
    return base + path.split("/").map(encodeURIComponent).join("/");
}

export function themeGridColumns(totalCards: number) {
    return Math.max(2, Math.ceil(Math.sqrt(totalCards)));
}

export function resolveModule(from: string, specifier: string, files: Record<string, string>): string {
    if (!specifier.startsWith("./") && !specifier.startsWith("../")) throw Error(`Import is outside the theme API: ${specifier}`);
    if (/[\\:\0?#]/.test(specifier)) throw Error(`Invalid theme import: ${specifier}`);
    const parts = from.split("/").slice(0, -1);
    for (const part of specifier.split("/")) {
        if (part === "..") {
            if (!parts.length) throw Error("Import escapes the theme");
            parts.pop();
        } else if (part && part !== ".") parts.push(part);
    }
    const path = parts.join("/");
    for (const candidate of [path, ...[".ts", ".tsx", ".js", ".jsx", ".json", "/index.ts", "/index.tsx", "/index.js", "/index.jsx"].map(ext => path + ext)]) {
        if (Object.hasOwn(files, candidate)) return candidate;
    }
    // Resource imports export a URL; CSS imports load a stylesheet in the isolated layer.
    if (/\.(css|svg|png|jpe?g|webp|gif|avif|ico|woff2?|ttf|otf|mp4|webm|mp3|ogg)$/i.test(path)) return path;
    throw Error(`Theme module not found: ${path}`);
}

export function validTokens(value: unknown): value is Record<string, string> {
    return !!value && typeof value === "object" && !Array.isArray(value) && Object.keys(value).length <= themeTokens.length &&
        Object.entries(value).every(([key, item]) => (themeTokens as readonly string[]).includes(key) && typeof item === "string" && item.length <= 2048 && !/[;{}\0]/.test(item));
}
