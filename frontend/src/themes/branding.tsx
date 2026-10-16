import type {Theme, ThemeBranding} from "./contract";
import {assetURL} from "./contract";
import {convertFileSrc} from "@tauri-apps/api/core";

export const startupTheme = (globalThis as typeof globalThis & { __MAPLOOM_THEME__?: Theme | null }).__MAPLOOM_THEME__ ?? null;

export function themeBranding(theme: Theme | null): ThemeBranding | null {
    const branding = theme?.manifest.branding;
    return branding ? {
        name: branding.name,
        logo: assetURL(convertFileSrc("", "maploom-theme") + encodeURIComponent(theme!.id) + "/", branding.logo)
    } : null;
}

export function ThemeBrand({branding}: { branding: ThemeBranding }) {
    return <div className="theme-brand"><img src={branding.logo} alt=""/><span>{branding.name}</span></div>;
}
