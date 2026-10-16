import {test} from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {readFileSync} from "node:fs";
import {createElement, type ReactNode} from "react";
import * as jsxRuntime from "react/jsx-runtime";
import {renderToStaticMarkup} from "react-dom/server";
import {ThemeBrand, themeBranding} from "../src/themes/branding";
import type {Appearance, Theme} from "../src/themes/contract";

test("branding resolves local artwork and renders an accessible name without markup injection", () => {
    assert.equal(themeBranding(null), null);
    const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
    Object.defineProperty(globalThis, "window", {configurable: true, value: {
        __TAURI_INTERNALS__: {convertFileSrc: (_path: string, protocol: string) => `http://${protocol}.localhost/`}
    }});
    try {
        const theme: Theme = {id: "БО [夜]", manifest: {
            format: "maploom/theme", api_version: 1, name: "Night", description: "", stylesheet: "style.css", entry: null,
            branding: {name: "Бесценный Опыт <test>", logo: "images/уроборос.svg"}
        }};
        const branding = themeBranding(theme)!;
        assert.equal(branding.logo, `http://maploom-theme.localhost/${encodeURIComponent(theme.id)}/images/${encodeURIComponent("уроборос.svg")}`);
        const html = renderToStaticMarkup(createElement(ThemeBrand, {branding}));
        assert.match(html, /Бесценный Опыт &lt;test&gt;/);
        assert.match(html, /alt=""/);
        assert.doesNotMatch(html, /Maploom|<test>/);
    } finally {
        if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
        else Reflect.deleteProperty(globalThis, "window");
    }
});
import {
    assetURL,
    isThemeSelected,
    resolveModule,
    resolveThemeMode,
    themeGridColumns,
    themeKey,
    validTokens
} from "../src/themes/contract";
import {compileModule} from "../src/themes/compiler";
import {ThemeSettings} from "../src/themes/ThemeSettings";

test("theme cards grow in square grids, counting the two built-in cards first", () => {
    for (const [count, columns] of [[2, 2], [3, 2], [4, 2], [5, 3], [9, 3], [10, 4], [16, 4], [17, 5], [25, 5], [26, 6]]) assert.equal(themeGridColumns(count), columns);
});

test("theme types override the base mode while adaptive and fallback appearances inherit it", () => {
    for (const fallback of ["dark", "light"] as const) {
        assert.equal(resolveThemeMode("dark", fallback), "dark");
        assert.equal(resolveThemeMode("light", fallback), "light");
        assert.equal(resolveThemeMode(undefined, fallback), fallback);
        assert.equal(resolveThemeMode(null, fallback), fallback);
    }
    const manifest = (id: string) => JSON.parse(readFileSync(new URL(`../../resources/theme-template/${id}/theme.json`, import.meta.url), "utf8"));
    assert.equal(resolveThemeMode(manifest("monokai").mode, "light"), "dark");
    assert.equal(resolveThemeMode(manifest("github").mode, "dark"), "light");
    assert.equal(resolveThemeMode(manifest("contour").mode, "dark"), "light");
    const variants = manifest("atlas").variants;
    assert.equal(resolveThemeMode(variants[0].mode, "light"), "dark");
    assert.equal(resolveThemeMode(variants[1].mode, "dark"), "light");
});

test("variants in one folder have independent card identities and saved selections", () => {
    const manifest = JSON.parse(readFileSync(new URL("../../resources/theme-template/atlas/theme.json", import.meta.url), "utf8"));
    const cards: Theme[] = manifest.variants.map((variant: { id: string; name: string; mode: string }) => ({
        id: "Мой Atlas [夜]", variant: variant.id, manifest: {...manifest, name: variant.name, mode: variant.mode}
    }));
    assert.equal(new Set(cards.map(themeKey)).size, 2);
    for (const selected of cards) {
        assert.deepEqual(cards.filter(card => isThemeSelected(card, selected.id, selected.variant)), [selected]);
    }
    assert.deepEqual(cards.filter(card => isThemeSelected(card, cards[0].id, null)), [cards[0]]);
    assert.equal(cards.some(card => isThemeSelected(card, cards[0].id, "missing")), false);
    assert.equal(isThemeSelected(cards[0], "another folder", cards[0].variant), false);
    const single: Theme = {...cards[0], variant: null, manifest: {...manifest, variants: []}};
    assert.equal(isThemeSelected(single, single.id, null), true);
    assert.equal(isThemeSelected(single, single.id, "dark"), false);
});

test("settings render both cards from one Atlas folder and mark only the chosen variant", () => {
    const manifest = JSON.parse(readFileSync(new URL("../../resources/theme-template/atlas/theme.json", import.meta.url), "utf8"));
    const themes: Theme[] = manifest.variants.map((variant: { id: string; name: string; mode: string }) => ({
        id: "Atlas", variant: variant.id, manifest: {...manifest, name: variant.name, mode: variant.mode}
    }));
    // This is a component render check; only Tauri's URL conversion is stubbed.
    const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
    Object.defineProperty(globalThis, "window", {
        configurable: true, value: {
            __TAURI_INTERNALS__: {convertFileSrc: (_path: string, protocol: string) => `http://${protocol}.localhost/`}
        }
    });
    let html: string;
    try {
        html = renderToStaticMarkup(createElement(ThemeSettings, {
            themes: {
                catalog: {directory: "themes", themes, warnings: []}, error: "", active: "Atlas", mode: "light",
                branding: null, working: false, generation: 0, reload() {
                }, open: async () => {
                }, example: async () => {
                }
            },
            selected: "Atlas", selectedVariant: "light", dark: true, accent: [137, 160, 109], onSelect() {
            }, t: key => key
        }));
    } finally {
        if (previousWindow) Object.defineProperty(globalThis, "window", previousWindow);
        else Reflect.deleteProperty(globalThis, "window");
    }
    assert.equal((html.match(/data-theme-id="Atlas"/g) ?? []).length, 2);
    assert.match(html, /data-theme-variant="dark" class="" aria-pressed="false"/);
    assert.match(html, /data-theme-variant="light" class="active" aria-pressed="true"/);
    assert.equal((html.match(/aria-pressed="true"/g) ?? []).length, 1);
    assert.match(html, /Atlas Dark/);
    assert.match(html, /Atlas Light/);
});

test("public imports resolve TSX, JS, JSON, assets, circular targets and nested relative modules", () => {
    const modules = {"entry.tsx": "", "widgets/index.tsx": "", "palette.json": "", "shared.js": ""};
    assert.equal(resolveModule("entry.tsx", "./widgets", modules), "widgets/index.tsx");
    assert.equal(resolveModule("widgets/index.tsx", "../palette.json", modules), "palette.json");
    assert.equal(resolveModule("widgets/index.tsx", "../entry.tsx", modules), "entry.tsx");
    assert.equal(resolveModule("entry.tsx", "./fonts/Текст.woff2", modules), "fonts/Текст.woff2");
    for (const path of ["../../secret", "@tauri-apps/api", "../app/bridge", "http://remote/module.js", "/app.ts", "./bad\\path.js"]) {
        assert.throws(() => resolveModule("entry.tsx", path, modules));
    }
});

test("asset URLs preserve Unicode, percent signs and nested paths without traversal", () => {
    const base = "http://maploom-theme.localhost/" + encodeURIComponent("Моя тема 100% ✨") + "/";
    const url = assetURL(base, "изображения/луна 1.svg");
    assert.equal(decodeURIComponent(new URL(url).pathname), "/Моя тема 100% ✨/изображения/луна 1.svg");
    for (const path of ["../secret", "https://remote/asset", "/absolute", "font\\name", "./image.png", "x?y"]) assert.throws(() => assetURL(base, path));
});

test("only public appearance tokens are accepted, with CSS statement injection rejected", () => {
    assert.ok(validTokens({"--accent": "#abc", "--theme-font": '"My Font", serif'}));
    for (const value of [{"--private": "x"}, {"--accent": "red;}body{display:none"}, {"--bg": 123}, [], null]) assert.equal(validTokens(value), false);
});

test("JS and TSX compile into isolated factories and communicate only through the supplied API", () => {
    for (const [name, source] of [["entry.js", "export default api => api.setTokens({'--accent':'#abc'});"],
        ["entry.tsx", "import type {ThemeAPI} from 'maploom:theme'; import {palette} from './palette'; export default function(api: ThemeAPI) { api.setTokens(palette); return <span>Moon</span>; }"]]) {
        let factory: ((require: (id: string) => unknown, module: {
            exports: unknown
        }, exports: unknown) => void) | undefined;
        vm.runInNewContext(compileModule(name, source), {
            __maploomRegister: (_id: string, value: typeof factory) => {
                factory = value;
            }
        });
        assert.ok(factory);
        const module = {exports: {} as { default?: (api: unknown) => unknown }};
        factory(id => id === "./palette" ? {palette: {"--accent": "#abc"}} : {jsx: () => "jsx works"}, module, module.exports);
        const tokens: unknown[] = [];
        const result = module.exports.default!({setTokens: (value: unknown) => tokens.push(value)});
        assert.equal(JSON.stringify(tokens), '[{"--accent":"#abc"}]');
        if (name.endsWith(".tsx")) assert.equal(result, "jsx works");
    }
    assert.throws(() => compileModule("broken.tsx", "export default function( {"));
});

test("Atlas TSX adapts accents, localizes its welcome decoration, hides it for maps, and cleans up", () => {
    const source = readFileSync(new URL("../../resources/theme-template/atlas/theme.tsx", import.meta.url), "utf8");
    type API = {
        root: HTMLElement; appearance: Appearance; setTokens(value: Record<string, string>): void;
        onAppearanceChange(listener: (appearance: Appearance) => void): () => void
    };
    type Factory = (require: (id: string) => unknown, module: { exports: unknown }, exports: unknown) => void;
    let factory: Factory | undefined;
    vm.runInNewContext(compileModule("theme.tsx", source), {
        __maploomRegister: (_id: string, value: Factory) => {
            factory = value;
        }
    });
    let rendered: ReactNode = null;
    let unmounted = false;
    const module = {exports: {} as { default: (api: API) => () => void }};
    factory!(id => {
        if (id === "react/jsx-runtime") return jsxRuntime;
        if (id === "react-dom/client") return {
            createRoot: () => ({
                render: (node: ReactNode) => {
                    rendered = node;
                }, unmount: () => {
                    unmounted = true;
                }
            })
        };
        throw Error(`Unexpected template dependency: ${id}`);
    }, module, module.exports);
    const tokens: Record<string, string>[] = [];
    const listeners = new Set<(appearance: Appearance) => void>();
    const initial: Appearance = {mode: "dark", accent: [137, 160, 109], locale: "en", screen: "welcome"};
    const dispose = module.exports.default({
        root: {} as HTMLElement, appearance: initial,
        setTokens: value => tokens.push(value),
        onAppearanceChange: listener => {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        }
    });
    assert.match(renderToStaticMarkup(rendered), /Every story has a horizon/);
    assert.match(renderToStaticMarkup(rendered), /prefers-reduced-motion/);
    const update = (appearance: Appearance) => {
        for (const listener of listeners) listener(appearance);
    };
    update({...initial, mode: "light", locale: "ru-RU", screen: "editor"});
    assert.equal(renderToStaticMarkup(rendered), "");
    assert.notEqual(tokens[0]["--accent"], tokens[1]["--accent"]);
    update({...initial, locale: "ru-RU"});
    assert.match(renderToStaticMarkup(rendered), /У каждой истории — свой горизонт/);
    assert.match(renderToStaticMarkup(rendered), />С<\/text>/);
    update({...initial, accent: [255, 255, 255]});
    assert.notEqual(tokens[0]["--accent"], tokens[3]["--accent"]);
    for (const value of tokens) assert.ok(validTokens(value));
    dispose();
    assert.equal(listeners.size, 0);
    assert.ok(unmounted);
});
