import {useState} from "react";
import {Download, FolderOpen, Package, Plus, Search, Trash2} from "lucide-react";
import type {AssetLookup} from "../map/model";
import type {PrefabEntry} from "../map/prefabs";
import {PrefabThumbnail} from "../rendering/PrefabPreview";

export function PrefabsPanel({
                                 entries,
                                 active,
                                 assets,
                                 canCreate,
                                 busy,
                                 t,
                                 onChoose,
                                 onCreate,
                                 onImport,
                                 onExport,
                                 onDelete
                             }: {
    entries: PrefabEntry[]; active?: string; assets: AssetLookup; canCreate: boolean; busy: boolean;
    t: (key: string) => string; onChoose: (key: string) => void; onCreate: () => void;
    onImport: () => void; onExport: (keys: string[]) => void; onDelete: (id: string) => void;
}) {
    const [query, setQuery] = useState("");
    const [source, setSource] = useState("all");
    const [page, setPage] = useState(0);
    const name = (entry: PrefabEntry) => entry.source === "default" ? t(entry.prefab.name) : entry.prefab.name;
    const filtered = entries.filter(p => (source === "all" || p.source === source) &&
        name(p).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase()));
    const pages = Math.max(1, Math.ceil(filtered.length / 48));
    const currentPage = Math.min(page, pages - 1);
    return <>
        <div className="library-controls">
            <div className="library-heading"><h2>{t("Prefabs")}</h2>
                <button className="button subtle compact" disabled={busy} onClick={onImport}><FolderOpen
                    size={14}/>{t("Import")}</button>
            </div>
            <button className="button subtle" disabled={!canCreate || busy} onClick={onCreate}
                    title={t("Prefab selection hint")}>
                <Plus size={15}/>{t("Create prefab")}
            </button>
            <p className="muted tiny">{t("Prefab selection hint")}</p>
            <label className="search-box"><Search size={15}/><input aria-label={t("Search prefabs")}
                                                                    placeholder={t("Search prefabs")} value={query}
                                                                    onChange={e => {
                                                                        setQuery(e.target.value);
                                                                        setPage(0);
                                                                    }}/></label>
            <div className="library-filters"><select aria-label={t("Prefab source")} value={source} onChange={e => {
                setSource(e.target.value);
                setPage(0);
            }}>
                <option value="all">{t("All prefabs")}</option>
                <option value="default">{t("Default prefabs")}</option>
                <option value="user">{t("My prefabs")}</option>
                <option value="pack">{t("Pack prefabs")}</option>
            </select></div>
            <div className="asset-count"><span>{filtered.length} {t("items")}</span>
                <button className="button subtle compact" disabled={!filtered.length || busy}
                        onClick={() => onExport(filtered.map(p => p.key))}>
                    <Download size={14}/>{t("Export prefabs")}
                </button>
            </div>
        </div>
        <div
            className="asset-gallery prefab-gallery">{filtered.slice(currentPage * 48, (currentPage + 1) * 48).map(entry =>
            <div
                key={entry.key} className={`asset-card ${active === entry.key ? "active" : ""}`}>
                <button className="asset-select" title={name(entry)} aria-pressed={active === entry.key}
                        onClick={() => onChoose(entry.key)}>
                    <PrefabThumbnail prefab={entry.prefab} assets={assets}/><strong>{name(entry)}</strong>
                    <small>{t("Components")}: {entry.prefab.items.length}</small>
                </button>
                <div className="prefab-card-actions">
                    <button title={t("Export prefab")} aria-label={`${t("Export prefab")}: ${name(entry)}`}
                            disabled={busy} onClick={() => onExport([entry.key])}><Download size={14}/></button>
                    {entry.source === "user" &&
                        <button title={t("Delete prefab")} aria-label={`${t("Delete prefab")}: ${entry.prefab.name}`}
                                disabled={busy} onClick={() => onDelete(entry.prefab.id)}><Trash2 size={14}/></button>}
                </div>
            </div>)}{!filtered.length &&
            <div className="library-empty"><Package size={30}/><strong>{t("No prefabs")}</strong>
                <p>{t("Prefab selection hint")}</p></div>}</div>
        {pages > 1 && <div className="gallery-pagination">
            <button disabled={!currentPage} onClick={() => setPage(currentPage - 1)}>‹</button>
            <span>{t("Page")} {currentPage + 1} / {pages}</span>
            <button disabled={currentPage + 1 >= pages} onClick={() => setPage(currentPage + 1)}>›</button>
        </div>}
        <p className="muted tiny prefab-export-hint">{t("Prefab export hint")}</p>
    </>;
}
