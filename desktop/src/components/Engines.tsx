// L'écran Moteurs : les projets récents à gauche ; au centre, les moteurs sur ce
// disque et la grille des versions d'Epic ; à droite, la version choisie.
import { Fragment, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
    api,
    branchOf,
    formatBytes,
    formatDay,
    formatSeconds,
    installing,
    jobFor,
    progressText,
    tilde,
    type AvailableEngine,
    type LocalEngine,
    type QueueJob,
    type RecentProject,
} from "../api";
import type { EngineState } from "../engines";
import type { Rate } from "../rates";
import EngineInspector from "./EngineInspector";
import EngineMatrix, { MatrixLegend } from "./EngineMatrix";

// Le résultat de la dernière action (lancer, ouvrir, désinstaller), sous les moteurs.
interface Status {
    text: string;
    error: boolean;
}

interface Props {
    // Les moteurs de ce disque et la liste d'Epic, tenus par App
    engines: EngineState;
    jobs: QueueJob[];
    rates: Record<number, Rate>;
}

export default function Engines({ engines, jobs, rates }: Props) {
    const { local, catalog, error: catalogError, refreshing } = engines;
    const [recent, setRecent] = useState<RecentProject[]>([]);
    const [status, setStatus] = useState<Status | null>(null);
    // Le moteur dont on demande confirmation avant de le supprimer, et celui qu'on supprime
    const [confirming, setConfirming] = useState<string | null>(null);
    const [removing, setRemoving] = useState<string | null>(null);
    const [folder, setFolder] = useState("");
    const [free, setFree] = useState<number | null>(null);
    // La version choisie dans la grille ; sans choix, celle qui s'installe ou la plus récente
    const [selected, setSelected] = useState<string | null>(null);
    // Refus de la file (déjà installé, dossier occupé…)
    const [notice, setNotice] = useState<string | null>(null);

    // La liste d'App s'affiche tout de suite ; on la relit chez Epic à chaque passage.
    useEffect(() => {
        void engines.refresh();
        api.enginesFolder().then(setFolder, () => setFolder(""));
    }, []);

    // Les projets récents : le moteur qui ouvre chacun dépend des moteurs installés.
    useEffect(() => {
        api.recentProjects().then(setRecent, () => setRecent([]));
    }, [local]);

    // La place libre change avec le dossier, et après chaque installation ou désinstallation.
    useEffect(() => {
        if (!folder) return;
        api.freeSpace(folder).then(setFree, () => setFree(null));
    }, [folder, local]);

    // Les moteurs enregistrés, et les projets récents : celui qui ouvre chacun en dépend.
    async function loadInstalled() {
        try {
            setRecent(await api.recentProjects());
        } catch {
            setRecent([]);
        }
    }

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
        await engines.reload();
    }

    async function install(version: string) {
        setSelected(version);
        try {
            await api.enqueue({ kind: "installEngine", version, folder });
            setNotice(null);
        } catch (e) {
            setNotice(String(e));
        }
    }

    const all = catalog?.engines ?? [];
    const current =
        selected ??
        jobs.map(installing).find((v) => v !== null) ??
        all[0]?.version ??
        null;
    const job = current ? jobFor(jobs, current) : undefined;
    const here = local.find((l) => l.version === current);

    return (
        <section className="engines">
            <header className="screen-head">
                <h1>Moteurs</h1>
                <span className="muted">Versions précompilées pour Linux</span>
                <div className="folder">
                    <span className="muted">Dossier</span>
                    <span className="folder-path" title={folder}>
            {tilde(folder)}
          </span>
                    {free !== null && (
                        <span className="muted">{formatBytes(free)} libres</span>
                    )}
                    <button onClick={() => void chooseFolder()}>Changer…</button>
                </div>
            </header>

            <aside className="engine-projects" aria-label="Projets récents">
                <h2>Projets récents</h2>
                {recent.length === 0 && (
                    <p className="muted">
                        Les projets auxquels tu ajoutes des assets apparaîtront ici.
                    </p>
                )}
                {recent.map((p) => (
                    <div key={p.path} className="project-card">
                        <div className="split">
              <span className="project-name" title={p.path}>
                {p.name}
              </span>
                            <span className="muted">
                {p.engine ? `UE ${p.engine}` : "Version inconnue"}
              </span>
                        </div>
                        <ProjectAction
                            project={p}
                            available={all}
                            jobs={jobs}
                            onOpen={() => openProject(p)}
                            onInstall={(version) => void install(version)}
                        />
                    </div>
                ))}
            </aside>

            <div className="engine-main">
                <section>
                    <h2>Sur ce disque</h2>
                    {local.length === 0 ? (
                        <p className="muted">Aucun moteur enregistré sur cet ordinateur.</p>
                    ) : (
                        <table className="installed">
                            <thead>
                            <tr>
                                <th scope="col">Version</th>
                                <th scope="col">Dossier</th>
                                <th scope="col">Origine</th>
                                <th scope="col">
                                    <span className="sr-only">Actions</span>
                                </th>
                            </tr>
                            </thead>
                            <tbody>
                            {local.map((e) => (
                                <Fragment key={e.dir}>
                                    <tr
                                        className={e.update || e.replacedBy ? "noted" : undefined}
                                    >
                                        <td className="engine-version">{e.version ?? "?"}</td>
                                        <td className="path" title={e.dir}>
                                            {tilde(e.dir)}
                                        </td>
                                        <td>
                        <span className={e.byUehub ? "origin ours" : "origin"}>
                          {e.byUehub ? "uehub" : "hors uehub"}
                        </span>
                                        </td>
                                        <td className="actions">
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
                                        </td>
                                    </tr>
                                    {(e.update || e.replacedBy) && (
                                        <tr className="engine-note-row">
                                            <td />
                                            <td colSpan={3}>
                                                <EngineNote
                                                    engine={e}
                                                    available={all}
                                                    jobs={jobs}
                                                    onInstall={(version) => void install(version)}
                                                />
                                            </td>
                                        </tr>
                                    )}
                                </Fragment>
                            ))}
                            </tbody>
                        </table>
                    )}
                    {status && (
                        <p className={status.error ? "error" : "muted"} role="status">
                            {status.text}
                        </p>
                    )}
                </section>

                <section>
                    <div className="section-head">
                        <h2>Publiées par Epic</h2>
                        {catalog?.eulaAccepted && (
                            <>
                                <MatrixLegend />
                                <button
                                    className="quiet"
                                    disabled={refreshing}
                                    onClick={() => void engines.refresh()}
                                >
                                    {refreshing ? "Rechargement…" : "Recharger"}
                                </button>
                            </>
                        )}
                    </div>
                    {notice && (
                        <p className="error" role="alert">
                            {notice}
                        </p>
                    )}
                    {catalogError ? (
                        <div className="message">
                            <p className="error">
                                Impossible de lire la liste d'Epic : {catalogError}
                            </p>
                            <button onClick={() => void engines.refresh()}>Réessayer</button>
                        </div>
                    ) : !catalog ? (
                        <p className="muted">Connexion à unrealengine.com…</p>
                    ) : !catalog.eulaAccepted ? (
                        <div className="message">
                            <p>
                                Epic demande d'accepter le contrat de licence d'Unreal Engine
                                (CLUF) avant de proposer le moteur. Accepte-le dans ton
                                navigateur, puis recharge la liste.
                            </p>
                            <div className="buttons">
                                <button
                                    className="primary"
                                    onClick={() => void api.openEulaPage()}
                                >
                                    Lire et accepter le CLUF
                                </button>
                                <button
                                    disabled={refreshing}
                                    onClick={() => void engines.refresh()}
                                >
                                    J'ai accepté, recharger
                                </button>
                            </div>
                        </div>
                    ) : (
                        <>
                            {/* Epic ne répond pas : la liste gardée reste utilisable. */}
                            {catalogError && (
                                <p className="error" role="status">
                                    Impossible de rafraîchir la liste, elle date du{" "}
                                    {formatSeconds(catalog.checkedAt)} : {catalogError}
                                </p>
                            )}
                            <EngineMatrix
                                engines={all}
                                local={local}
                                jobs={jobs}
                                selected={current}
                                onSelect={setSelected}
                            />
                        </>
                    )}
                </section>
            </div>

            <EngineInspector
                engine={all.find((e) => e.version === current)}
                installed={here}
                newer={all.find((e) => e.version === here?.update)}
                newerJob={here?.update ? jobFor(jobs, here.update) : undefined}
                job={job}
                rate={job ? rates[job.id] : undefined}
                folder={folder}
                free={free}
                onInstall={(version) => void install(version)}
                onSelect={setSelected}
                onLaunch={launch}
                onPause={(id) => void api.pauseJob(id)}
                onResume={(id) => void api.resumeJob(id)}
                onRemove={(id) => void api.removeJob(id)}
            />
        </section>
    );
}

interface ProjectActionProps {
    project: RecentProject;
    available: AvailableEngine[];
    jobs: QueueJob[];
    onOpen: () => void;
    onInstall: (version: string) => void;
}

// Sous la ligne d'un moteur : la version plus récente de sa branche qu'Epic a publiée,
// ou celle qui le remplace déjà sur ce disque.
function EngineNote(props: {
    engine: LocalEngine;
    available: AvailableEngine[];
    jobs: QueueJob[];
    onInstall: (version: string) => void;
}) {
    const { engine, available, jobs, onInstall } = props;
    const { update, replacedBy } = engine;
    if (update) {
        const date = available.find((a) => a.version === update)?.date;
        return (
            <p className="engine-note update">
                La {update} est sortie{date && ` le ${formatDay(date)}`}.
                {jobFor(jobs, update) ? (
                    " Elle est dans la file."
                ) : (
                    <button className="primary" onClick={() => onInstall(update)}>
                        Installer {update}
                    </button>
                )}
            </p>
        );
    }
    if (!replacedBy) return null;
    return (
        <p className="engine-note muted">
            Remplacé par la {replacedBy}, qui ouvre maintenant tes projets{" "}
            {branchOf(replacedBy)}.{engine.byUehub && " Tu peux le désinstaller."}
        </p>
    );
}

// Ouvrir le projet, ou installer la dernière version de sa branche si Epic la propose.
function ProjectAction(props: ProjectActionProps) {
    const { project, available, jobs, onOpen, onInstall } = props;
    if (project.opensWith)
        return <button onClick={onOpen}>Ouvrir avec {project.opensWith}</button>;
    if (!project.engine) return null;
    // La liste d'Epic est triée du plus récent au plus ancien : la première est la bonne.
    const candidate = available.find((e) =>
        e.version.startsWith(`${project.engine}.`),
    );
    if (!candidate)
        return (
            <p className="muted small">
                Aucun moteur {project.engine} sur ce disque.
            </p>
        );
    const job = jobFor(jobs, candidate.version);
    if (job)
        return (
            <p className="muted small">
                UE {candidate.version} :{" "}
                {job.status.state === "running"
                    ? progressText(job.progress)
                    : job.status.state === "failed"
                        ? "échec, voir la file"
                        : job.status.state === "paused"
                            ? "en pause"
                            : "en attente"}
            </p>
        );
    return (
        <>
            <p className="muted small">
                Aucun moteur {project.engine} sur ce disque.
            </p>
            <button className="primary" onClick={() => onInstall(candidate.version)}>
                Installer {candidate.version}
            </button>
        </>
    );
}