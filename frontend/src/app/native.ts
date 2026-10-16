import {invoke as tauriInvoke, type InvokeArgs} from "@tauri-apps/api/core";

/** The native host installs this capability only in the top-level window.
 * A sandboxed theme cannot obtain it, even if WebView injects its IPC helpers. */
export function invoke<T>(command: string, args?: InvokeArgs): Promise<T> {
    const shell = (globalThis as typeof globalThis & { __MAPLOOM_INVOKE__?: typeof tauriInvoke }).__MAPLOOM_INVOKE__;
    return (shell ?? tauriInvoke)<T>(command, args);
}
