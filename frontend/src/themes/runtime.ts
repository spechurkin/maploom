import {type Appearance, assetURL, type ThemePackage, validTokens} from "./contract";
import bootstrapRaw from "./bootstrap.js?raw";

const bootstrapSource = bootstrapRaw.replace(/\r\n/g, "\n");

export type ThemeRuntime = { update(appearance: Appearance): void; dispose(): void };

/** All entry code runs in an opaque-origin sandbox. The sole returned capability
 * changes public CSS tokens; no document, filesystem or editor command crosses it. */
export async function activateTheme(root: HTMLElement, bundle: ThemePackage, base: string,
                                    appearance: Appearance, onError: (error: string) => void,
                                    signal: AbortSignal): Promise<ThemeRuntime> {
    let disposed = false;
    let active = false;
    let port: MessagePort | undefined;
    let frame: HTMLIFrameElement | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let abortPending: (() => void) | undefined;
    let lastTokens: Record<string, string> = {};
    let latestAppearance = appearance;
    const tokenStyle = document.createElement("style");
    tokenStyle.dataset.themeTokens = "v1";
    const link = document.createElement("link");
    link.rel = "stylesheet";
    // Load before applying, so an invalid package cannot leave half a theme behind.
    link.media = "not all";

    function dispose() {
        if (disposed) return;
        disposed = true;
        active = false;
        clearTimeout(timer);
        abortPending?.();
        port?.postMessage({type: "dispose"});
        port?.close();
        frame?.remove();
        link.remove();
        tokenStyle.remove();
        root.removeAttribute("data-external-theme");
        signal.removeEventListener("abort", dispose);
    }

    function applyTokens() {
        tokenStyle.textContent = `[data-theme-part="app"][data-external-theme] {${Object.entries(lastTokens).map(([key, value]) => `${key}:${value} !important;`).join("")}}`;
    }

    function fail(error: unknown) {
        if (disposed) return;
        dispose();
        onError(String(error));
    }

    signal.addEventListener("abort", dispose, {once: true});
    if (signal.aborted) {
        dispose();
        throw Error("Theme activation cancelled");
    }
    try {
        if (bundle.theme.manifest.stylesheet) {
            link.href = assetURL(base, bundle.theme.manifest.stylesheet) + `?v=${Date.now()}`;
            await new Promise<void>((resolve, reject) => {
                timer = setTimeout(() => reject(Error("Theme stylesheet timed out")), 15000);
                abortPending = () => reject(Error("Theme activation cancelled"));
                link.onload = () => {
                    clearTimeout(timer);
                    resolve();
                };
                link.onerror = () => reject(Error("Cannot load theme stylesheet"));
                document.head.append(link);
            });
        }
        if (bundle.theme.manifest.entry) {
            const {default: source} = await import("./sandboxSource");
            if (disposed) throw Error("Theme activation cancelled");
            frame = document.createElement("iframe");
            frame.className = "theme-decoration";
            frame.dataset.themePart = "decoration";
            frame.sandbox.add("allow-scripts");
            frame.setAttribute("aria-hidden", "true");
            frame.tabIndex = -1;
            frame.title = bundle.theme.manifest.name;
            // A restrictive inner policy also blocks Tauri IPC, network and nested frames.
            const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(bootstrapSource));
            const hash = btoa(String.fromCharCode(...new Uint8Array(digest)));
            frame.srcdoc = `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'sha256-${hash}' blob:; style-src 'unsafe-inline' maploom-theme: http://maploom-theme.localhost; img-src data: blob: maploom-theme: http://maploom-theme.localhost; font-src data: maploom-theme: http://maploom-theme.localhost; media-src maploom-theme: http://maploom-theme.localhost; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"><style>html,body,#theme-root{margin:0;width:100%;height:100%;overflow:hidden;background:transparent}*{box-sizing:border-box}</style></head><body><div id="theme-root"></div><script>${bootstrapSource}</script></body></html>`;
            const channel = new MessageChannel();
            port = channel.port1;
            await new Promise<void>((resolve, reject) => {
                timer = setTimeout(() => reject(Error("Theme script timed out")), 15000);
                abortPending = () => reject(Error("Theme activation cancelled"));
                port!.onmessage = event => {
                    if (disposed || !event.data || typeof event.data !== "object") return;
                    if (event.data.type === "tokens") {
                        if (!validTokens(event.data.tokens)) {
                            fail("Unsupported theme tokens");
                            return;
                        }
                        lastTokens = event.data.tokens;
                        if (active) applyTokens();
                    } else if (event.data.type === "ready") {
                        clearTimeout(timer);
                        resolve();
                    } else if (event.data.type === "error") {
                        const error = String(event.data.error).slice(0, 4096);
                        if (active) fail(error); else reject(Error(error));
                    }
                };
                frame!.onload = () => {
                    frame!.onload = () => fail("Theme sandbox navigated away");
                    frame!.contentWindow?.postMessage({
                        type: "maploom:theme:bootstrap", source,
                        init: {type: "maploom:theme:init", bundle, base, appearance: latestAppearance}
                    }, "*", [channel.port2]);
                };
                root.append(frame!);
            });
        }
        if (disposed) throw Error("Theme activation cancelled");
        abortPending = undefined;
        active = true;
        root.dataset.externalTheme = bundle.theme.id;
        link.media = "all";
        document.head.append(tokenStyle);
        applyTokens();
        return {
            dispose,
            update(value) {
                latestAppearance = value;
                if (!disposed) port?.postMessage({type: "appearance", appearance: value});
            },
        };
    } catch (error) {
        dispose();
        throw error;
    }
}
