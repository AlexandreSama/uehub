import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    formatBytes,
    type Account,
    type Asset,
    type Progress,
    type RecentProject,
    type Summary,
} from "./api";
import Login from "./components/Login";
import TopBar, { type Filter } from "./components/TopBar";
import AssetGrid from "./components/AssetGrid";
import Details from "./components/Details";
import StatusBar, { type Job, type Status } from "./components/StatusBar";

type Screen = "starting" | "login" | "loading" | "library";

export default function App() {
    const [screen, setScreen] = useState<Screen>("starting");
    const [account, setAccount] = useState<Account | null>(null);
    const [assets, setAssets] = useState<Asset[]>([]);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [selectedId, setSelectedId] = useState<string | null>(null);
    const [query, setQuery] = useState("");
    const [filter, setFilter] = useState<Filter>("all");
    const [overwrite, setOverwrite] = useState(false);
    const [job, setJob] = useState<Job | null>(null);
    const [status, setStatus] = useState<Status | null>(null);
    const [recent, setRecent] = useState<RecentProject[]>([]);
    // Vrai si l'utilisateur a cliqué sur Annuler pendant le téléchargement en cours.
    const cancelRequested = useRef(false);

    async function loadRecent() {
        try {
            setRecent(await api.recentProjects());
        } catch {
            setRecent([]);
        }
    }

    async function loadLibrary() {
        void loadRecent();
        setScreen("loading");
        setLoadError(null);
        try {
            setAssets(await api.library());
        } catch (e) {
            setLoadError(String(e));
        }
        setScreen("library");
    }

    // Au démarrage : reprendre la session enregistrée, sinon afficher la connexion.
    useEffect(() => {
        let cancelled = false;
        api
            .session()
            .then((found) => {
                if (cancelled) return;
                if (found) {
                    setAccount(found);
                    loadLibrary();
                } else {
                    setScreen("login");
                }
            })
            .catch(() => !cancelled && setScreen("login"));
        return () => {
            cancelled = true;
        };
    }, []);

    async function handleLogin(code: string) {
        setAccount(await api.login(code));
        await loadLibrary();
    }

    async function handleLogout() {
        await api.logout();
        setAccount(null);
        setAssets([]);
        setSelectedId(null);
        setStatus(null);
        setScreen("login");
    }

    const visible = useMemo(() => {
        const q = query.trim().toLowerCase();
        return assets.filter(
            (a) =>
                (filter === "all" || a.kind === filter) &&
                a.title.toLowerCase().includes(q),
        );
    }, [assets, query, filter]);

    // Le détail suit la grille : un asset masqué par la recherche n'y reste pas affiché.
    const selected = visible.find((a) => a.id === selectedId) ?? null;

    // Lance une action longue en suivant sa progression dans la barre du bas.
    async function run(
        asset: Asset,
        verb: string,
        action: (onProgress: (p: Progress) => void) => Promise<Summary>,
    ) {
        cancelRequested.current = false;
        setStatus(null);
        setJob({ title: asset.title, progress: null, cancelling: false });
        try {
            const s = await action((progress) =>
                setJob((job) => job && { ...job, progress }),
            );
            const text = `${verb} : ${s.files} fichiers, ${formatBytes(s.bytes)} dans ${s.dir}`;
            setStatus({ tone: "ok", text: s.note ? `${text}. ${s.note}` : text });
        } catch (e) {
            // Une annulation demandée n'est pas une erreur : pas de rouge.
            const tone = cancelRequested.current ? "ok" : "error";
            setStatus({ tone, text: String(e) });
        } finally {
            setJob(null);
        }
    }

    async function handleCancel() {
        cancelRequested.current = true;
        setJob((job) => job && { ...job, cancelling: true });
        await api.cancelJob();
    }

    async function handleDownload() {
        if (!selected) return;
        const folder = await open({
            directory: true,
            title: "Dossier où ranger l'asset",
        });
        if (!folder) return;
        await run(selected, "Téléchargé", (onProgress) =>
            api.download(selected.id, folder, onProgress),
        );
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
        await run(selected, "Ajouté", (onProgress) =>
            api.addToProject(selected.id, uproject, overwrite, onProgress),
        );
        // Rust a mis ce projet en tête de liste : on recharge.
        await loadRecent();
    }

    async function handleForget(uproject: string) {
        await api.forgetProject(uproject);
        await loadRecent();
    }

    if (screen === "starting") return <div className="splash" />;
    if (screen === "login")
        return <Login onOpenPage={api.openLoginPage} onSubmit={handleLogin} />;

    return (
        <div className="app">
            <TopBar
                account={account}
                count={assets.length}
                query={query}
                onQuery={setQuery}
                filter={filter}
                onFilter={setFilter}
                onLogout={handleLogout}
            />
            <AssetGrid
                loading={screen === "loading"}
                error={loadError}
                onRetry={loadLibrary}
                assets={visible}
                selectedId={selectedId}
                onSelect={setSelectedId}
            />
            <Details
                asset={selected}
                recent={recent}
                busy={job !== null}
                overwrite={overwrite}
                onOverwrite={setOverwrite}
                onAdd={handleAdd}
                onForget={handleForget}
                onDownload={handleDownload}
            />
            <StatusBar job={job} status={status} onCancel={handleCancel} />
        </div>
    );
}