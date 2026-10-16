// This is bundled as an independent runtime with its own React and compiler.
// It must never import the application bridge, state, components or document model.
import React from "react";
import * as jsx from "react/jsx-runtime";
import * as reactDOM from "react-dom/client";
import {compileModule} from "./compiler";
import {type Appearance, assetURL, resolveModule, type ThemeAPI, type ThemePackage, validTokens} from "./contract";

type Factory = (require: (id: string) => unknown, module: { exports: unknown }, exports: unknown) => void;
let initialized = false;
window.addEventListener("message", event => {
    if (event.source !== parent || initialized || event.data?.type !== "maploom:theme:init" || !event.ports[0]) return;
    initialized = true;
    const port = event.ports[0];
    const {bundle, base} = event.data as { bundle: ThemePackage; base: string };
    let appearance: Appearance = Object.freeze(event.data.appearance);
    const listeners = new Set<(value: Appearance) => void>();
    const fail = (error: unknown) => port.postMessage({type: "error", error: String(error).slice(0, 4096)});
    window.addEventListener("error", event => fail(event.message));
    window.addEventListener("unhandledrejection", event => fail(event.reason));
    const api: ThemeAPI = Object.freeze({
        version: 1,
        root: document.getElementById("theme-root")!,
        get appearance() {
            return appearance;
        },
        asset: (path: string) => assetURL(base, path),
        setTokens(tokens: Record<string, string>) {
            if (!validTokens(tokens)) throw Error("Unsupported theme tokens");
            port.postMessage({type: "tokens", tokens});
        },
        onAppearanceChange(listener: (value: Appearance) => void) {
            listeners.add(listener);
            return () => {
                listeners.delete(listener);
            };
        },
    });
    let cleanup: (() => void) | void;
    port.onmessage = event => {
        if (event.data?.type === "appearance") {
            appearance = Object.freeze(event.data.appearance);
            try {
                for (const listener of listeners) listener(appearance);
            } catch (error) {
                fail(error);
            }
        } else if (event.data?.type === "dispose") {
            try {
                cleanup?.();
            } finally {
                port.close();
            }
        }
    };
    const factories = new Map<string, Factory>();
    const cache = new Map<string, { exports: unknown }>();
    (globalThis as typeof globalThis & {
        __maploomRegister: (id: string, factory: Factory) => void
    }).__maploomRegister = (id, factory) => factories.set(id, factory);
    const builtins: Record<string, unknown> = {
        react: React, "react/jsx-runtime": jsx, "react/jsx-dev-runtime": jsx,
        "react-dom/client": reactDOM, "maploom:theme": {defineTheme: (activate: unknown) => activate}
    };
    const styles: Promise<void>[] = [];

    function requireModule(from: string, specifier: string): unknown {
        if (Object.hasOwn(builtins, specifier)) return builtins[specifier];
        const id = resolveModule(from, specifier, bundle.modules);
        if (cache.has(id)) return cache.get(id)!.exports;
        const module = {exports: {} as unknown};
        cache.set(id, module); // Circular local imports follow CommonJS semantics.
        if (id.endsWith(".json")) module.exports = JSON.parse(bundle.modules[id]);
        else if (id.endsWith(".css")) {
            const link = document.createElement("link");
            link.rel = "stylesheet";
            link.href = api.asset(id);
            styles.push(new Promise((resolve, reject) => {
                link.onload = () => resolve();
                link.onerror = () => reject(Error(`Cannot load ${id}`));
            }));
            document.head.append(link);
        } else if (factories.has(id)) factories.get(id)!(name => requireModule(id, name), module, module.exports);
        else module.exports = api.asset(id);
        return module.exports;
    }

    void (async () => {
        for (const [id, source] of Object.entries(bundle.modules)) {
            if (id.endsWith(".json")) continue;
            const url = URL.createObjectURL(new Blob([compileModule(id, source)], {type: "text/javascript"}));
            try {
                await new Promise<void>((resolve, reject) => {
                    const script = document.createElement("script");
                    script.src = url;
                    script.onload = () => {
                        script.remove();
                        resolve();
                    };
                    script.onerror = () => {
                        script.remove();
                        reject(Error(`Cannot load ${id}`));
                    };
                    document.head.append(script);
                });
            } finally {
                URL.revokeObjectURL(url);
            }
        }
        const entry = requireModule("", `./${bundle.theme.manifest.entry}`) as { default?: unknown };
        const activate = typeof entry === "function" ? entry : entry.default;
        if (typeof activate !== "function") throw Error("Theme entry must export a default activate(api) function");
        cleanup = await activate(api);
        if (cleanup !== undefined && typeof cleanup !== "function") throw Error("Theme cleanup must be a function");
        await Promise.all(styles);
        port.postMessage({type: "ready"});
    })().catch(fail);
});
