// L'écran Bibliothèque : les filtres à gauche, la liste des assets au centre,
// l'inspecteur de l'asset choisi à droite.
import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    compareVersions,
    removalText,
    type AddedAsset,
    type Asset,
    type JobRequest,
    type RecentProject,
} from "../api";
import AssetList from "./AssetList";
import { SearchIcon } from "./Common";
import Details from "./Details";
import Filters, { type KindFilter, type Tracked } from "./Filters";

type Sort = "fab" | "title";

// Ce qu'on affiche sous les boutons de l'inspecteur : un refus, ou le bilan d'un retrait.
export interface Notice {
    text: string;
    error: boolean;
}

interface Props {
    loading: boolean;
    error: string | null;
    onRetry: () => void;
    // Relit la bibliothèque sans vider la liste ; rejette si Fab ne répond pas
    onReload: () => Promise<void>;
    assets: Asset[];
    recent: RecentProject[];
    // Ce que uehub a ajouté aux projets récents
    added: AddedAsset[];
    // Relit les projets récents et leurs fiches : un ajout met son projet en tête
    onProjectsChange: () => Promise<void>;
}

export default function Library(props: Props) {
    const { assets, recent, added, onReload, onProjectsChange } = props;
    const [query, setQuery] = useState("");
    const [kind, setKind] = useState<KindFilter>("all");
    const [tracked, setTracked] = useState<Tracked>("all");
    const [version, setVersion] = useState<string | null>(null);
    const [sort, setSort] = useState<Sort>("fab");
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [overwrite, setOverwrite] = useState(false);
    const [notice, setNotice] = useState<Notice | null>(null);
    const [reloading, setReloading] = useState(false);
    // Ce qui n'a pas marché dans l'en-tête : le rechargement, ou l'ouverture de Fab
    const [headError, setHeadError] = useState<string | null>(null);

    // Les versions d'Unreal présentes dans la bibliothèque, de la plus ancienne à la plus récente.
    const versions = useMemo(
        () => [...new Set(assets.flatMap((a) => a.engines))].sort(compareVersions),
        [assets],
    );

    // Les assets ajoutés à au moins un projet, et ceux qui ont une mise à jour quelque part.
    const addedIds = useMemo(() => new Set(added.map((a) => a.assetId)), [added]);
    const updateIds = useMemo(
        () => new Set(added.filter((a) => a.update).map((a) => a.assetId)),
        [added],
    );

    const visible = useMemo(() => {
        const q = query.trim().toLowerCase();
        const shown = assets.filter(
            (a) =>
                (kind === "all" || a.kind === kind) &&
                (!version || a.engines.includes(version)) &&
                (tracked === "all" ||
                    (tracked === "added" ? addedIds : updateIds).has(a.id)) &&
                a.title.toLowerCase().includes(q),
        );
        return sort === "title"
            ? [...shown].sort((a, b) => a.title.localeCompare(b.title, "fr"))
            : shown;
    }, [assets, query, kind, version, tracked, addedIds, updateIds, sort]);

    // L'inspecteur suit la liste : un asset masqué par les filtres n'y reste pas affiché.
    const selected = visible.find((a) => a.id === selectedId) ?? null;

    function select(id: string) {
        setSelectedId(id);
        setNotice(null);
    }

    async function enqueue(request: JobRequest) {
        try {
            await api.enqueue(request);
            setNotice(null);
        } catch (e) {
            setNotice({ text: String(e), error: true });
        }
    }

    // Un asset acheté sur Fab depuis le démarrage n'apparaît qu'après un rechargement.
    async function handleReload() {
        setReloading(true);
        setHeadError(null);
        try {
            await onReload();
        } catch (e) {
            setHeadError(`Impossible de recharger la bibliothèque : ${e}`);
        }
        setReloading(false);
    }

    async function openFab(assetId: string | null) {
        try {
            await api.openFab(assetId);
        } catch (e) {
            const text = String(e);
            if (assetId) setNotice({ text, error: true });
            else setHeadError(text);
        }
    }

    async function handleDownload() {
        if (!selected) return;
        const folder = await open({
            directory: true,
            title: "Dossier où ranger l'asset",
        });
        if (!folder) return;
        await enqueue({ kind: "download", assetId: selected.id, folder });
    }

    // `path` vient de la liste des projets récents ; sinon on demande le projet.
    async function handleAdd(path: string | null) {
        if (!selected) return;
        const uproject =
            path ??
            (await open({
                title: "Projet Unreal",
                filters: [{ name: "Projet Unreal", extensions: ["uproject"] }],
            }));
        if (!uproject) return;
        await enqueue({
            kind: "addToProject",
            assetId: selected.id,
            uproject,
            overwrite,
        });
        await onProjectsChange();
    }

    // Une mise à jour, c'est un nouvel ajout par-dessus l'ancien : il écrase forcément.
    async function handleUpdate(uproject: string) {
        if (!selected) return;
        await enqueue({
            kind: "addToProject",
            assetId: selected.id,
            uproject,
            overwrite: true,
        });
        await onProjectsChange();
    }

    // L'inspecteur demande confirmation avant : retirer efface des fichiers du projet.
    async function handleRemove(uproject: string) {
        if (!selected) return;
        try {
            const removal = await api.removeAsset(uproject, selected.id);
            setNotice({ text: removalText(removal), error: false });
        } catch (e) {
            setNotice({ text: String(e), error: true });
        }
        await onProjectsChange();
    }

    async function handleForget(uproject: string) {
        await api.forgetProject(uproject);
        await onProjectsChange();
    }

    return (
        <section className="library">
            <header className="screen-head">
                <h1>Bibliothèque</h1>
                <span className="muted">{assets.length} assets Fab</span>
                <button
                    className="quiet reload"
                    disabled={reloading || props.loading}
                    onClick={() => void handleReload()}
                >
                    {reloading ? "Rechargement…" : "Recharger"}
                </button>
                <button className="quiet" onClick={() => void openFab(null)}>
                    Ouvrir Fab
                </button>
                <label className="search">
                    <SearchIcon />
                    <input
                        type="search"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                        placeholder="Rechercher un asset"
                        aria-label="Rechercher un asset"
                    />
                </label>
                <label className="sort">
                    Trier par
                    <select
                        value={sort}
                        onChange={(e) => setSort(e.target.value as Sort)}
                    >
                        <option value="fab">Ordre de Fab</option>
                        <option value="title">Titre</option>
                    </select>
                </label>
                {headError && (
                    <p className="head-error" role="alert">
                        {headError}
                    </p>
                )}
            </header>
            <Filters
                assets={assets}
                versions={versions}
                recent={recent}
                kind={kind}
                onKind={setKind}
                version={version}
                onVersion={setVersion}
                tracked={tracked}
                onTracked={setTracked}
                addedCount={addedIds.size}
                updateCount={updateIds.size}
            />
            <AssetList
                loading={props.loading}
                error={props.error}
                onRetry={props.onRetry}
                assets={visible}
                updates={updateIds}
                selectedId={selectedId}
                onSelect={select}
            />
            <Details
                asset={selected}
                added={added.filter((a) => a.assetId === selected?.id)}
                versions={versions}
                recent={recent}
                notice={notice}
                overwrite={overwrite}
                onOverwrite={setOverwrite}
                onAdd={handleAdd}
                onDownload={handleDownload}
                onUpdate={handleUpdate}
                onRemove={handleRemove}
                onOpenFab={() => void openFab(selected?.id ?? null)}
                onForget={handleForget}
            />
        </section>
    );
}