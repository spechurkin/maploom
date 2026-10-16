import {desktop} from "../app/bridge";
import type {useThemes} from "./useThemes";
import {Check} from "lucide-react";
import type {CSSProperties} from "react";
import {isThemeSelected, themeGridColumns, themeKey} from "./contract";
import {ThemePreview} from "./ThemePreview";

export function ThemeSettings({themes, selected, selectedVariant, dark, accent, onSelect, t}: {
    themes: ReturnType<typeof useThemes>;
    selected?: string | null;
    selectedVariant?: string | null;
    dark: boolean;
    accent: readonly number[];
    onSelect(id: string | null, dark?: boolean, variant?: string | null): void;
    t(key: string): string
}) {
    const unavailable = !!selected && !themes.catalog.themes.some(theme => isThemeSelected(theme, selected, selectedVariant));
    const columns = themeGridColumns(2 + themes.catalog.themes.length + Number(unavailable));
    return <div className="setting-section external-themes">
        <h3>{t("Theme")}</h3>
        <div className="theme-options" role="group" aria-label={t("Theme")}
             data-theme-columns={columns}
             style={{"--theme-columns": columns, "--preview-accent": `rgb(${accent.join(",")})`} as CSSProperties}>
            {[true, false].map(mode => <button key={String(mode)} className={!selected && dark === mode ? "active" : ""}
                                               aria-pressed={!selected && dark === mode}
                                               onClick={() => onSelect(null, mode)}>
                <span className={`theme-preview ${mode ? "dark-preview" : "light-preview"}`}/>
                <strong>{t(mode ? "Dark" : "Light theme")}</strong>
                {!selected && dark === mode && <Check size={15}/>}
            </button>)}
            {themes.catalog.themes.map(theme => <button key={themeKey(theme)} data-theme-id={theme.id}
                                                        data-theme-variant={theme.variant ?? undefined}
                                                        className={isThemeSelected(theme, selected, selectedVariant) ? "active" : ""}
                                                        aria-pressed={isThemeSelected(theme, selected, selectedVariant)}
                                                        disabled={!desktop || themes.working}
                                                        title={theme.manifest.description || theme.manifest.name}
                                                        onClick={() => onSelect(theme.id, undefined, theme.variant)}>
                <ThemePreview theme={theme} dark={dark} accent={accent} generation={themes.generation}/>
                <strong>{theme.manifest.name}</strong>
                {isThemeSelected(theme, selected, selectedVariant) && <Check size={15}/>}
            </button>)}
            {unavailable &&
                <div className="theme-unavailable"><strong>{selected}</strong><span>{t("Theme unavailable")}</span>
                </div>}
        </div>
        <p className="info-note">{t(desktop ? "Themes folder hint" : "Themes desktop hint")}</p>
        <div className="theme-actions">
            <button className="button" disabled={!desktop || themes.working}
                    onClick={() => void themes.open()}>{t("Open themes folder")}</button>
            <button className="button" disabled={!desktop || themes.working}
                    onClick={themes.reload}>{t("Reload themes")}</button>
            <button className="button" disabled={!desktop || themes.working}
                    onClick={() => void themes.example()}>{t("Create example theme")}</button>
        </div>
        {themes.working && <p role="status">{t("Loading theme")}</p>}
        {themes.error && <p className="theme-error" role="alert">{t("Theme fallback hint")} {themes.error}</p>}
        {themes.catalog.warnings.length > 0 && <details className="theme-error">
            <summary>{t("Theme loading warnings")}</summary>
            {themes.catalog.warnings.map((warning, index) => <p key={index}>{warning}</p>)}</details>}
    </div>;
}
