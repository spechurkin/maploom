import {AlertCircle, CheckCheck, Download, ExternalLink, LoaderCircle, RefreshCw} from "lucide-react";
import {version} from "../../../package.json";
import {desktop} from "../app/bridge";
import type {useUpdates} from "../app/useUpdates";

type Props = {
    appName?: string;
    updates: ReturnType<typeof useUpdates>;
    t: (key: string) => string;
    dirty: boolean;
    busy: boolean;
    onInstall: () => Promise<void>
};

export function UpdatesPanel({updates: u, t, dirty, busy, onInstall, appName = "Maploom"}: Props) {
    const available = u.info?.status === "available";
    const retryInstall = !!u.error && u.failedAction === "install" && available;
    const title = u.checking ? "Update checking" : u.error ? retryInstall ? "Update install failed" : "Update check failed" : available ? "Update available title"
        : u.info?.status === "current" ? "Update current" : u.info?.status === "unpublished" ? "Update unavailable" : "Update stay current";
    const StatusIcon = u.checking ? LoaderCircle : u.error ? AlertCircle : available ? Download : u.info?.status === "current" ? CheckCheck : RefreshCw;
    return <section className="updates-panel" aria-live="polite">
        <div className="update-app-version"><span>{appName}</span><span>{t("Version")} {version}</span></div>
        <div className={`update-status-card ${available && !u.error ? "has-update" : ""}`}>
            <span className={`update-status-icon ${u.checking ? "spin" : ""}`}><StatusIcon size={24}/></span>
            <div className="update-status-copy">
                <h3>{t(title)}{available && !u.error && !u.checking && <> {u.info?.version}</>}</h3>
                <p>{t(!desktop ? "Update desktop only" : u.error ? "Update failed" : available
                    ? u.info?.install_blocked ? "Update manual hint" : dirty ? "Update save hint" : "Update ready hint"
                    : u.info?.status === "unpublished" ? "Update try later" : "Update automatic hint")}</p>
            </div>
        </div>
        {available && !u.error && u.info?.notes && <details className="update-release-notes">
            <summary>{t("Update what is new")}</summary>
            <p className="update-notes">{u.info.notes}</p>
        </details>}
        <div className="update-actions">
            {available && (!u.error || retryInstall) &&
                <button className="button primary" disabled={busy || u.checking || u.installing}
                        onClick={() => void (u.info?.install_blocked ? u.openRelease() : onInstall())}>
                    {u.info?.install_blocked ? <ExternalLink size={16}/> : <Download size={16}/>}
                    {t(retryInstall ? "Update retry" : u.info?.install_blocked ? "Update download manual" : dirty ? "Update save and install" : "Update install")}
                </button>}
            <button className={`button ${u.error && !retryInstall ? "primary" : "subtle"}`}
                    disabled={!desktop || u.checking || u.installing}
                    onClick={() => void u.check()}><RefreshCw size={15} className={u.checking ? "spin" : ""}/>
                {t(u.checking ? "Update checking" : u.error && !retryInstall ? "Update retry" : "Check for updates")}
            </button>
        </div>
    </section>;
}
