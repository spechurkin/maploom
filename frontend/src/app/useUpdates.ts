import {useCallback, useEffect, useRef, useState} from "react";
import {desktop} from "./bridge";
import {checkUpdate, installUpdate, openUpdateRelease, type UpdateInfo, type UpdateProgress} from "./updates";

export function useUpdates() {
    const [info, setInfo] = useState<UpdateInfo | null>(null);
    const [checking, setChecking] = useState(false);
    const [installing, setInstalling] = useState(false);
    const [progress, setProgress] = useState<UpdateProgress | null>(null);
    const [error, setError] = useState("");
    const [failedAction, setFailedAction] = useState<"check" | "install">("check");
    const [dismissed, setDismissed] = useState<string | null>(null);
    const inFlight = useRef(false);
    const check = useCallback(async (manual = true) => {
        if (!desktop || inFlight.current) return;
        inFlight.current = true;
        setChecking(true);
        setError("");
        try {
            const next = await checkUpdate();
            setInfo(next);
            if (manual) setDismissed(null);
        } catch (e) {
            if (manual) {
                setFailedAction("check");
                setError(String(e));
            }
        } finally {
            inFlight.current = false;
            setChecking(false);
        }
    }, []);
    useEffect(() => {
        if (!desktop || import.meta.env.DEV) return;
        const start = setTimeout(() => void check(false), 5000);
        const timer = setInterval(() => void check(false), 6 * 60 * 60 * 1000);
        return () => {
            clearTimeout(start);
            clearInterval(timer);
        };
    }, [check]);
    const install = async () => {
        if (!info?.version || inFlight.current) return;
        inFlight.current = true;
        setInstalling(true);
        setProgress(null);
        setError("");
        try {
            await installUpdate(info.version, setProgress);
        } catch (e) {
            setFailedAction("install");
            setError(String(e));
        } finally {
            inFlight.current = false;
            setInstalling(false);
        }
    };
    const openRelease = async () => {
        try {
            await openUpdateRelease();
        } catch {
            setError("Update browser failed");
        }
    };
    return {
        info, checking, installing, progress, error, failedAction, check, install, openRelease,
        notified: info?.status === "available" && info.version !== dismissed,
        dismiss: () => setDismissed(info?.version ?? null)
    };
}
