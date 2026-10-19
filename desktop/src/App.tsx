import { useEffect, useState } from "react";
import {
    api,
    onQueueChange,
    type Account,
    type Asset,
    type AddedAsset,
    type QueueJob,
    type RecentProject,
} from "./api";
import { useTransferRates } from "./rates";
import { useAppUpdate } from "./update";
import Login from "./components/Login";
import Rail, { type View } from "./components/Rail";
import Library from "./components/Library";
import Engines from "./components/Engines";
import QueueDock from "./components/QueueDock";
import QueueDrawer from "./components/QueueDrawer";

type Screen = "starting" | "login" | "loading" | "library";

export default function App() {
    const [screen, setScreen] = useState<Screen>("starting");
    const [view, setView] = useState<View>("library");
    const [account, setAccount] = useState<Account | null>(null);
    const [assets, setAssets] = useState<Asset[]>([]);
    const [loadError, setLoadError] = useState<string | null>(null);
    const [recent, setRecent] = useState<RecentProject[]>([]);
    // Ce que uehub a ajouté aux projets récents, avec les mises à jour
    const [added, setAdded] = useState<AddedAsset[]>([]);
    const [jobs, setJobs] = useState<QueueJob[]>([]);
    const [queueOpen, setQueueOpen] = useState(false);
    const rates = useTransferRates(jobs);
    const update = useAppUpdate();

    async function loadRecent() {
        try {
            setRecent(await api.recentProjects());
        } catch {
            setRecent([]);
        }
    }

    async function loadAdded() {
        try {
            setAdded(await api.addedAssets());
        } catch {
            setAdded([]);
        }
    }

    // Les projets récents et leurs fiches : retirer un projet retire aussi ses fiches.
    async function loadProjects() {
        await loadRecent();
        await loadAdded();
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
        // Les mises à jour se lisent dans la bibliothèque : on compare une fois qu'elle est là.
        void loadAdded();
    }

    // Relit la bibliothèque sans vider la liste. Une erreur remonte à la Bibliothèque,
    // qui l'affiche dans son en-tête.
    async function reloadLibrary() {
        setAssets(await api.library());
        setLoadError(null);
        void loadAdded();
    }

    // Chaque ajout terminé laisse une fiche dans son projet : on relit les fiches.
    const addsDone = jobs.filter(
        (j) => j.request.kind === "addToProject" && j.status.state === "done",
    ).length;
    useEffect(() => {
        void loadAdded();
    }, [addsDone]);

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
        setAdded([]);
        setView("library");
        setScreen("login");
    }

    if (screen === "starting") return <div className="splash" />;
    if (screen === "login")
        return <Login onOpenPage={api.openLoginPage} onSubmit={handleLogin} />;

    // Le redémarrage d'une mise à jour arrête ce qui tourne : on prévient avant.
    const queueBusy = jobs.some(
        (j) => j.status.state === "running" || j.status.state === "waiting",
    );

    return (
        <div className="app">
            <Rail
                view={view}
                onView={setView}
                account={account}
                onLogout={handleLogout}
                update={update}
                queueBusy={queueBusy}
            />
            {view === "engines" ? (
                <Engines jobs={jobs} rates={rates} />
            ) : (
                <Library
                    loading={screen === "loading"}
                    error={loadError}
                    onRetry={loadLibrary}
                    onReload={reloadLibrary}
                    assets={assets}
                    recent={recent}
                    added={added}
                    onProjectsChange={loadProjects}
                />
            )}
            {queueOpen && (
                <QueueDrawer
                    jobs={jobs}
                    rates={rates}
                    onPause={(id) => void api.pauseJob(id)}
                    onResume={(id) => void api.resumeJob(id)}
                    onRemove={(id) => void api.removeJob(id)}
                    onResumeAll={() => void api.resumeAll()}
                    onClearFinished={() => void api.clearFinished()}
                />
            )}
            <QueueDock
                jobs={jobs}
                rates={rates}
                open={queueOpen}
                onToggle={() => setQueueOpen(!queueOpen)}
                onPause={(id) => void api.pauseJob(id)}
                onResumeAll={() => void api.resumeAll()}
            />
        </div>
    );
}