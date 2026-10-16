import {Channel} from "@tauri-apps/api/core";
import {invoke} from "./native";

export type UpdateInfo = {
    status: "available" | "current" | "unpublished";
    version: string | null;
    notes: string | null;
    install_blocked: string | null;
};
export type UpdateProgress = { phase: "downloading" | "installing"; downloaded: number; total: number | null };

export const checkUpdate = () => invoke<UpdateInfo>("update_check");
export const openUpdateRelease = () => invoke<void>("update_open_release");

export function installUpdate(version: string, onProgress: (event: UpdateProgress) => void) {
    const progress = new Channel<UpdateProgress>();
    progress.onmessage = onProgress;
    return invoke<void>("update_install", {version, progress});
}
