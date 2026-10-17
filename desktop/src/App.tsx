import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    formatBytes,
    type Account,
    type Asset,
    type Progress,
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

    async function loadLibrary() {
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
        setStatus(null);
        setJob({ title: asset.title, progress: null });
        try {
            const s = await action((progress) =>
                setJob({ title: asset.title, progress }),
            );
            const text = `${verb} : ${s.files} fichiers, ${formatBytes(s.bytes)} dans ${s.dir}`;
            setStatus({ tone: "ok", text: s.note ? `${text}. ${s.note}` : text });
        } catch (e) {
            setStatus({ tone: "error", text: String(e) });
        } finally {
            setJob(null);
        }
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

    async function handleAdd() {
        if (!selected) return;
        const uproject = await open({
            title: "Projet Unreal",
            filters: [{ name: "Projet Unreal", extensions: ["uproject"] }],
        });
        if (!uproject) return;
        await run(selected, "Ajouté", (onProgress) =>
            api.addToProject(selected.id, uproject, overwrite, onProgress),
        );
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
                busy={job !== null}
                overwrite={overwrite}
                onOverwrite={setOverwrite}
                onAdd={handleAdd}
                onDownload={handleDownload}
            />
            <StatusBar job={job} status={status} />
        </div>
    );
}