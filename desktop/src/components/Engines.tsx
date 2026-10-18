// L'écran Moteurs : les moteurs déjà enregistrés (à lancer ou désinstaller), les projets
// récents à ouvrir avec le bon moteur, et les versions précompilées qu'Epic propose
// pour Linux. « Installer » met une installation dans la file.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    formatBytes,
    progressText,
    type AvailableEngine,
    type EngineCatalog,
    type LocalEngine,
    type QueueJob,
    type RecentProject,
} from "../api";
import { ProgressBar } from "./Common";

const day = new Intl.DateTimeFormat("fr-FR", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
});

// "2026-09-22" → "22 sept. 2026"
function formatDay(date: string | null): string {
    return date ? day.format(new Date(date)) : "";
}

// La plus récente de chaque branche (5.8, 5.7…). La liste arrive déjà triée.
function latestPerBranch(engines: AvailableEngine[]): AvailableEngine[] {
    const seen = new Set<string>();
    return engines.filter((e) => {
        const branch = e.version.split(".").slice(0, 2).join(".");
        if (seen.has(branch)) return false;
        seen.add(branch);
        return true;
    });
}

// Le job de la file qui installe cette version, s'il n'est pas terminé.
function jobFor(jobs: QueueJob[], version: string): QueueJob | undefined {
    return jobs.find(
        (j) =>
            j.request.kind === "installEngine" &&
            j.request.version === version &&
            j.status.state !== "done",
    );
}

// Le résultat de la dernière action (lancer, ouvrir, désinstaller), sous les moteurs.
interface Status {
    text: string;
    error: boolean;
}

export default function Engines({ jobs }: { jobs: QueueJob[] }) {
    const [local, setLocal] = useState<LocalEngine[]>([]);
    const [recent, setRecent] = useState<RecentProject[]>([]);
    const [status, setStatus] = useState<Status | null>(null);
    // Le moteur dont on demande confirmation avant de le supprimer, et celui qu'on supprime
    const [confirming, setConfirming] = useState<string | null>(null);
    const [removing, setRemoving] = useState<string | null>(null);
    const [catalog, setCatalog] = useState<EngineCatalog | null>(null);
    const [catalogError, setCatalogError] = useState<string | null>(null);
    const [folder, setFolder] = useState("");
    const [showAll, setShowAll] = useState(false);
    // Refus de la file (déjà installé, dossier occupé…)
    const [notice, setNotice] = useState<string | null>(null);

    // Les moteurs enregistrés, et les projets récents : celui qui ouvre chacun en dépend.
    async function loadInstalled() {
        try {
            setLocal(await api.localEngines());
        } catch {
            setLocal([]);
        }
        try {
            setRecent(await api.recentProjects());
        } catch {
            setRecent([]);
        }
    }

    // La liste d'Epic passe par une session unrealengine.com : quelques secondes.
    async function loadCatalog() {
        setCatalog(null);
        setCatalogError(null);
        try {
            setCatalog(await api.availableEngines());
        } catch (e) {
            setCatalogError(String(e));
        }
    }

    useEffect(() => {
        void loadCatalog();
        api.enginesFolder().then(setFolder, () => setFolder(""));
    }, []);

    // Relit les moteurs enregistrés à l'ouverture, puis à chaque installation terminée.
    const finished = jobs.filter(
        (j) => j.request.kind === "installEngine" && j.status.state === "done",
    ).length;
    useEffect(() => {
        void loadInstalled();
    }, [finished]);

    // Le dossier choisi est retenu dans les réglages pour les prochains lancements.
    async function chooseFolder() {
        const chosen = await open({
            directory: true,
            title: "Dossier des moteurs",
            defaultPath: folder || undefined,
        });
        if (!chosen) return;
        setFolder(chosen);
        try {
            await api.setEnginesFolder(chosen);
        } catch (e) {
            setNotice(String(e));
        }
    }

    async function run(action: Promise<void>, done: string) {
        try {
            await action;
            setStatus({ text: done, error: false });
        } catch (e) {
            setStatus({ text: String(e), error: true });
        }
    }

    function launch(engine: LocalEngine) {
        const text = `UE ${engine.version ?? "?"} démarre : sa fenêtre peut mettre une minute à s'ouvrir.`;
        void run(api.launchEngine(engine.dir), text);
    }

    function openProject(project: RecentProject) {
        const text = `${project.name} s'ouvre avec UE ${project.opensWith} : la fenêtre peut mettre une minute à s'afficher.`;
        void run(api.openProject(project.path), text);
    }

    async function uninstall(engine: LocalEngine) {
        setConfirming(null);
        setRemoving(engine.dir);
        const text = `UE ${engine.version ?? "?"} est désinstallé.`;
        await run(api.removeEngine(engine.dir), text);
        setRemoving(null);
        await loadInstalled();
    }

    async function install(version: string) {
        try {
            await api.enqueue({ kind: "installEngine", version, folder });
            setNotice(null);
        } catch (e) {
            setNotice(String(e));
        }
    }

    const all = catalog?.engines ?? [];
    const shown = showAll ? all : latestPerBranch(all);

    return (
        <section className="engines">
            <div className="engines-page">
                <header className="engines-head">
                    <h2>Moteurs Unreal</h2>
                    <p className="muted">
                        Les versions précompilées pour Linux, publiées par Epic. Chacune
                        s'installe dans son dossier et s'enregistre pour que tes projets la
                        retrouvent.
                    </p>
                </header>

                <h3>Installés</h3>
                {local.length === 0 ? (
                    <p className="muted">Aucun moteur enregistré sur cet ordinateur.</p>
                ) : (
                    <ul className="engine-list">
                        {local.map((e) => (
                            <li key={e.dir} className="engine-row local">
                                <span className="engine-version">UE {e.version ?? "?"}</span>
                                <span className="muted engine-dir" title={e.dir}>
                  {e.dir}
                </span>
                                <span className="engine-action">
                  {removing === e.dir ? (
                      <span className="muted">Suppression…</span>
                  ) : confirming === e.dir ? (
                      <>
                          <span>Supprimer ce dossier ?</span>
                          <button
                              className="danger"
                              onClick={() => void uninstall(e)}
                          >
                              Supprimer
                          </button>
                          <button
                              className="quiet"
                              onClick={() => setConfirming(null)}
                          >
                              Annuler
                          </button>
                      </>
                  ) : (
                      <>
                          <button onClick={() => launch(e)}>Lancer</button>
                          {e.byUehub && (
                              <button
                                  className="quiet"
                                  onClick={() => setConfirming(e.dir)}
                              >
                                  Désinstaller
                              </button>
                          )}
                      </>
                  )}
                </span>
                            </li>
                        ))}
                    </ul>
                )}
                {status && (
                    <p className={status.error ? "error" : "muted"} role="status">
                        {status.text}
                    </p>
                )}

                {recent.length > 0 && (
                    <>
                        <h3>Projets récents</h3>
                        <ul className="engine-list">
                            {recent.map((p) => (
                                <li key={p.path} className="engine-row project">
                  <span className="engine-project" title={p.path}>
                    {p.name}
                  </span>
                                    <span className="muted">
                    {p.engine ? `UE ${p.engine}` : "Version inconnue"}
                  </span>
                                    <span className="engine-action">
                    <ProjectAction
                        project={p}
                        available={all}
                        jobs={jobs}
                        onOpen={() => openProject(p)}
                        onInstall={(version) => void install(version)}
                    />
                  </span>
                                </li>
                            ))}
                        </ul>
                    </>
                )}

                <div className="engines-available">
                    <h3>Proposés par Epic</h3>
                    {catalog && catalog.eulaAccepted && (
                        <button className="quiet" onClick={() => void loadCatalog()}>
                            Recharger
                        </button>
                    )}
                </div>

                {catalogError ? (
                    <div className="engines-message">
                        <p className="error">
                            Impossible de lire la liste d'Epic : {catalogError}
                        </p>
                        <button onClick={() => void loadCatalog()}>Réessayer</button>
                    </div>
                ) : !catalog ? (
                    <p className="muted">Connexion à unrealengine.com…</p>
                ) : !catalog.eulaAccepted ? (
                    <div className="engines-message">
                        <p>
                            Epic demande d'accepter le contrat de licence d'Unreal Engine
                            (CLUF) avant de proposer le moteur. Accepte-le dans ton
                            navigateur, puis recharge la liste.
                        </p>
                        <div className="engines-buttons">
                            <button
                                className="primary"
                                onClick={() => void api.openEulaPage()}
                            >
                                Lire et accepter le CLUF
                            </button>
                            <button onClick={() => void loadCatalog()}>
                                J'ai accepté, recharger
                            </button>
                        </div>
                    </div>
                ) : (
                    <>
                        <p className="engines-folder">
                            <span className="muted">Dossier :</span>
                            <span className="engine-dir" title={folder}>
                {folder}
              </span>
                            <button className="quiet" onClick={() => void chooseFolder()}>
                                Changer…
                            </button>
                        </p>
                        <p className="muted engines-space">
                            Prévois environ 2,8 fois la taille du zip en espace libre : le
                            zip, puis le moteur extrait. Le zip est supprimé à la fin.
                        </p>
                        {notice && (
                            <p className="error" role="alert">
                                {notice}
                            </p>
                        )}
                        <ul className="engine-list">
                            {shown.map((e) => {
                                const job = jobFor(jobs, e.version);
                                const installed = local.some((l) => l.version === e.version);
                                return (
                                    <li key={e.version} className="engine-row">
                                        <span className="engine-version">UE {e.version}</span>
                                        <span className="muted">{formatDay(e.date)}</span>
                                        <span className="engine-size">{formatBytes(e.size)}</span>
                                        <span className="engine-action">
                      {job ? (
                          <JobState job={job} />
                      ) : installed ? (
                          <span className="muted">Installé</span>
                      ) : (
                          <button onClick={() => void install(e.version)}>
                              Installer
                          </button>
                      )}
                    </span>
                                    </li>
                                );
                            })}
                        </ul>
                        {all.length > shown.length || showAll ? (
                            <button className="quiet" onClick={() => setShowAll(!showAll)}>
                                {showAll
                                    ? "Seulement la dernière de chaque branche"
                                    : `Toutes les versions (${all.length})`}
                            </button>
                        ) : null}
                    </>
                )}
            </div>
        </section>
    );
}

// Une version déjà dans la file : son avancement, ou son état.
function JobState({ job }: { job: QueueJob }) {
    if (job.status.state === "running")
        return (
            <span className="engine-progress">
        <span>{progressText(job.progress)}</span>
        <ProgressBar progress={job.progress} />
      </span>
        );
    if (job.status.state === "failed")
        return (
            <span className="error" title={job.status.error}>
        Échec, voir la file
      </span>
        );
    return (
        <span className="muted">
      {job.status.state === "paused" ? "En pause" : "En attente"}
    </span>
    );
}

interface ProjectActionProps {
    project: RecentProject;
    available: AvailableEngine[];
    jobs: QueueJob[];
    onOpen: () => void;
    onInstall: (version: string) => void;
}

// Ouvrir le projet, ou installer la dernière version de sa branche si Epic la propose.
function ProjectAction(props: ProjectActionProps) {
    const { project, available, jobs, onOpen, onInstall } = props;
    if (project.opensWith)
        return <button onClick={onOpen}>Ouvrir avec UE {project.opensWith}</button>;
    if (!project.engine) return null;
    // La liste d'Epic est triée du plus récent au plus ancien : la première est la bonne.
    const candidate = available.find((e) =>
        e.version.startsWith(`${project.engine}.`),
    );
    if (!candidate)
        return (
            <span className="muted">UE {project.engine} n'est pas installé</span>
        );
    const job = jobFor(jobs, candidate.version);
    if (job) return <JobState job={job} />;
    return (
        <button onClick={() => onInstall(candidate.version)}>
            Installer UE {candidate.version}
        </button>
    );
}