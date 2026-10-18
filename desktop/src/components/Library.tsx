// L'écran Bibliothèque : les filtres à gauche, la liste des assets au centre,
// l'inspecteur de l'asset choisi à droite.
import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    compareVersions,
    type Asset,
    type AddedAsset,
    type JobRequest,
    type RecentProject,
} from "../api";
import AssetList from "./AssetList";
import { SearchIcon } from "./Common";
import Details from "./Details";
import Filters, { type KindFilter, type Tracked } from "./Filters";

type Sort = "fab" | "title";

interface Props {
    loading: boolean;
    error: string | null;
    onRetry: () => void;
    assets: Asset[];
    recent: RecentProject[];
    // Ce que uehub a ajouté aux projets récents
    added: AddedAsset[];
    // Relit les projets récents et leurs fiches : un ajout met son projet en tête
    onProjectsChange: () => Promise<void>;
}

export default function Library(props: Props) {
    const { assets, recent, added, onProjectsChange } = props;
    const [query, setQuery] = useState("");
    const [kind, setKind] = useState<KindFilter>("all");
    const [tracked, setTracked] = useState<Tracked>("all");
    const [version, setVersion] = useState<string | null>(null);
    const [sort, setSort] = useState<Sort>("fab");
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [overwrite, setOverwrite] = useState(false);
    // Refus de la file (déjà en file, version introuvable…), affiché sous les boutons.
    const [notice, setNotice] = useState<string | null>(null);

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
            setNotice(String(e));
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

    async function handleForget(uproject: string) {
        await api.forgetProject(uproject);
        await onProjectsChange();
    }

    return (
        <section className="library">
            <header className="screen-head">
                <h1>Bibliothèque</h1>
                <span className="muted">{assets.length} assets Fab</span>
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
                onForget={handleForget}
                onDownload={handleDownload}
                onUpdate={handleUpdate}
            />
        </section>
    );
}