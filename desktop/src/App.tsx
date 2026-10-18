import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    onQueueChange,
    type Account,
    type Asset,
    type JobRequest,
    type QueueJob,
    type RecentProject,
} from "./api";
import Login from "./components/Login";
import TopBar, { type Filter } from "./components/TopBar";
import AssetGrid from "./components/AssetGrid";
import Details from "./components/Details";
import QueueBar from "./components/QueueBar";
import QueueDrawer from "./components/QueueDrawer";

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
    const [recent, setRecent] = useState<RecentProject[]>([]);
    const [jobs, setJobs] = useState<QueueJob[]>([]);
    const [queueOpen, setQueueOpen] = useState(false);
    // Refus de la file (déjà en file, version introuvable…), affiché sous les boutons.
    const [notice, setNotice] = useState<string | null>(null);

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

    // La file vit dans Rust : on la lit une fois, puis on suit ses changements.
    useEffect(() => {
        const stop = onQueueChange(setJobs);
        api.jobs().then(setJobs);
        return () => {
            stop.then((unlisten) => unlisten());
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
        setNotice(null);
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

    function handleSelect(id: string) {
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
                onSelect={handleSelect}
            />
            <Details
                asset={selected}
                recent={recent}
                notice={notice}
                overwrite={overwrite}
                onOverwrite={setOverwrite}
                onAdd={handleAdd}
                onForget={handleForget}
                onDownload={handleDownload}
            />
            {queueOpen && (
                <QueueDrawer
                    jobs={jobs}
                    onPause={(id) => void api.pauseJob(id)}
                    onResume={(id) => void api.resumeJob(id)}
                    onRemove={(id) => void api.removeJob(id)}
                    onResumeAll={() => void api.resumeAll()}
                    onClearFinished={() => void api.clearFinished()}
                />
            )}
            <QueueBar
                jobs={jobs}
                open={queueOpen}
                onToggle={() => setQueueOpen(!queueOpen)}
                onResumeAll={() => void api.resumeAll()}
            />
        </div>
    );
}