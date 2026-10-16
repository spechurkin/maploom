import {useEffect, useMemo, useState} from "react";
import {convertFileSrc} from "@tauri-apps/api/core";
import {invoke} from "../app/native";
import {assetURL, resolveThemeMode, type Theme} from "./contract";

/** A miniature public shell in a script-free, opaque-origin document. */
export function ThemePreview({theme, dark: baseDark, accent, generation}: {
    theme: Theme;
    dark: boolean;
    accent: readonly number[];
    generation: number
}) {
    const dark = resolveThemeMode(theme.manifest.mode, baseDark ? "dark" : "light") === "dark";
    const [stylesheet, setStylesheet] = useState("");
    const [status, setStatus] = useState("loading");
    useEffect(() => {
        let alive = true;
        setStatus("loading");
        invoke<string>("theme_preview", {id: theme.id}).then(css => {
            if (alive) {
                setStylesheet(css);
                setStatus("ready");
            }
        }, () => {
            if (alive) {
                setStylesheet("");
                setStatus("error");
            }
        });
        return () => {
            alive = false;
        };
    }, [theme.id, generation]);
    const color = accent.join(",");
    const html = useMemo(() => {
        const escape = (value: string) => value.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");
        const base = convertFileSrc("", "maploom-theme") + encodeURIComponent(theme.id) + "/";
        const href = theme.manifest.stylesheet ? assetURL(base, theme.manifest.stylesheet) : base;
        const cssDirectory = href.slice(0, href.lastIndexOf('/') + 1);
        const css = stylesheet.replace(/</g, "\\3c ");
        return `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'none'; style-src 'unsafe-inline' maploom-theme: http://maploom-theme.localhost; img-src data: maploom-theme: http://maploom-theme.localhost; font-src data: maploom-theme: http://maploom-theme.localhost; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri maploom-theme: http://maploom-theme.localhost"><base href="${escape(cssDirectory)}"><style>
html,body{margin:0;width:100%;height:100%;overflow:hidden}*{box-sizing:border-box}
[data-theme-part="app"]{--accent:rgb(${color});--bg:${dark ? "#101218" : "#e6e8eb"};--panel:${dark ? "#242932" : "#f9fafb"};--panel-alt:${dark ? "#1c2028" : "#f0f2f4"};--canvas:${dark ? "#101218" : "#e6e8eb"};--text:${dark ? "#e3e5e9" : "#28313b"};--line:${dark ? "#2a2f38" : "#d9dee4"};height:100%;background:var(--bg);color:var(--text);font-family:var(--theme-font,system-ui)}
[data-theme-part="workspace"]{display:flex;height:100%}[data-theme-part="tools"]{width:32%;height:100%;background:var(--panel);border-right:1px solid var(--line)}[data-theme-part="canvas"]{position:relative;flex:1;background:var(--canvas)}.preview-map{position:absolute;inset:20% 12%;background:var(--accent)}
</style><style>${css}</style></head><body><div class="studio ${dark ? "dark" : "light"}" data-theme-part="app" data-theme-api="1" data-theme-mode="${dark ? "dark" : "light"}" data-external-theme=""><div data-theme-part="workspace"><aside data-theme-part="tools"></aside><main data-theme-part="canvas"><div class="preview-map"></div></main></div></div></body></html>`;
    }, [theme.id, theme.manifest.stylesheet, dark, color, stylesheet]);
    if (status !== "ready") return <span className={`theme-preview ${dark ? "dark-preview" : "light-preview"}`}
                                         data-preview-status={status}/>;
    // Remount for changed CSS/mode: never retain the previous srcdoc's policy or
    // document while a browser defers an iframe navigation.
    return <iframe key={html} className="theme-preview external-theme-preview" srcDoc={html} sandbox=""
                   title={theme.manifest.name}
                   aria-hidden="true" tabIndex={-1} data-preview-status={status}/>;
}
