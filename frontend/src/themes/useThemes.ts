import {useEffect, useRef, useState} from "react";
import {convertFileSrc} from "@tauri-apps/api/core";
import {invoke} from "../app/native";
import {desktop} from "../app/bridge";
import type {Preferences} from "../map/model";
import {resolveThemeMode, type Appearance, type Theme, type ThemeCatalog, type ThemeMode, type ThemePackage} from "./contract";
import {activateTheme, type ThemeRuntime} from "./runtime";
import {startupTheme, themeBranding} from "./branding";

export function useThemes(preferences: Preferences | undefined, projectOpen: boolean) {
    const [catalog, setCatalog] = useState<ThemeCatalog>({directory: "", themes: [], warnings: []});
    const [error, setError] = useState("");
    const [loaded, setLoaded] = useState<{
        id: string;
        variant: string | null;
        generation: number;
        mode?: ThemeMode | null;
        theme: Theme;
    } | null>(null);
    const [generation, setGeneration] = useState(0);
    const [working, setWorking] = useState(false);
    const runtime = useRef<ThemeRuntime | null>(null);
    const baseAppearance: Appearance = {
        mode: preferences?.dark === false ? "light" : "dark",
        accent: preferences?.accent ?? [137, 160, 109],
        locale: preferences?.locale ?? "en",
        screen: projectOpen ? "editor" : "welcome"
    };
    const id = preferences ? preferences.theme : startupTheme?.id;
    const variant = preferences ? preferences.theme_variant ?? null : startupTheme?.variant ?? null;
    const active = loaded?.id === id && loaded?.variant === variant && loaded?.generation === generation ? loaded : null;
    const initialTheme = !error && generation === 0 && startupTheme && startupTheme.id === id &&
        (startupTheme.variant ?? null) === (variant ?? startupTheme.manifest.variants?.[0]?.id ?? null) ? startupTheme : null;
    const appearance: Appearance = {...baseAppearance, mode: resolveThemeMode(active?.mode ?? initialTheme?.manifest.mode, baseAppearance.mode)};
    const latest = useRef(baseAppearance);
    latest.current = baseAppearance;
    useEffect(() => {
        if (!desktop) return;
        let alive = true;
        invoke<ThemeCatalog>("theme_catalog").then(value => {
            if (alive && value) setCatalog(value);
        }, failure => {
            if (alive) setError(String(failure));
        });
        return () => {
            alive = false;
        };
    }, [generation]);
    useEffect(() => {
        setError("");
        setLoaded(null);
        if (!desktop || !id) return;
        const controller = new AbortController();
        const root = document.querySelector<HTMLElement>('[data-theme-part="app"]');
        if (!root) return;
        setWorking(true);
        void invoke<ThemePackage>("theme_package", {id, variant}).then(async bundle => {
            if (controller.signal.aborted) return;
            const base = convertFileSrc("", "maploom-theme") + encodeURIComponent(id) + "/";
            const themeAppearance = () => ({
                ...latest.current,
                mode: resolveThemeMode(bundle.theme.manifest.mode, latest.current.mode)
            });
            const instance = await activateTheme(root, bundle, base, themeAppearance(), failure => {
                if (!controller.signal.aborted) {
                    setError(failure);
                    setLoaded(null);
                }
            }, controller.signal);
            if (controller.signal.aborted) instance.dispose();
            else {
                runtime.current = instance;
                instance.update(themeAppearance());
                setLoaded({id, variant, generation, mode: bundle.theme.manifest.mode, theme: bundle.theme});
            }
        }).catch(failure => {
            if (!controller.signal.aborted) setError(String(failure));
        })
            .finally(() => {
                if (!controller.signal.aborted) setWorking(false);
            });
        return () => {
            controller.abort();
            runtime.current?.dispose();
            runtime.current = null;
            setWorking(false);
        };
    }, [id, variant, generation]);
    useEffect(() => {
        runtime.current?.update(appearance);
    }, [appearance.mode, appearance.locale, appearance.screen, appearance.accent.join(",")]);

    async function openDirectory() {
        try {
            await invoke("theme_open_directory");
        } catch (failure) {
            setError(String(failure));
        }
    }

    async function operate(command: string) {
        setWorking(true);
        try {
            await invoke(command);
            setGeneration(n => n + 1);
        } catch (failure) {
            setError(String(failure));
        } finally {
            setWorking(false);
        }
    }

    return {
        catalog,
        error,
        active: active?.id ?? "",
        mode: appearance.mode,
        branding: themeBranding(active?.theme ?? initialTheme),
        working,
        generation,
        reload: () => setGeneration(n => n + 1),
        open: openDirectory,
        example: () => operate("theme_create_example")
    };
}
