// L'écran Moteurs : les projets récents à gauche ; au centre, les moteurs sur ce
// disque et la grille des versions d'Epic ; à droite, la version choisie.
import { Fragment, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  branchOf,
  formatBytes,
  formatSeconds,
  installing,
  jobFor,
  progressText,
  releaseText,
  tilde,
  type AvailableEngine,
  type LocalEngine,
  type QueueJob,
  type RecentProject,
} from "../api";
import type { EngineState } from "../engines";
import { t } from "../i18n";
import type { Rate } from "../rates";
import EngineInspector from "./EngineInspector";
import EngineMatrix, { MatrixLegend } from "./EngineMatrix";
import LogButton from "./LogButton";

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

  // Le dossier choisi est retenu dans les réglages pour les prochains lancements.
  async function chooseFolder() {
    const chosen = await open({
      directory: true,
      title: t("engines.folder_dialog"),
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
    const text = t("engines.launching", { version: engine.version ?? "?" });
    void run(api.launchEngine(engine.dir), text);
  }

  function openProject(project: RecentProject) {
    const text = t("engines.opening", {
      project: project.name,
      version: project.opensWith ?? "?",
    });
    void run(api.openProject(project.path), text);
  }

  async function uninstall(engine: LocalEngine) {
    setConfirming(null);
    setRemoving(engine.dir);
    const text = t("engines.uninstalled", { version: engine.version ?? "?" });
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
        <h1>{t("screens.engines")}</h1>
        <span className="muted">{t("engines.subtitle")}</span>
        <div className="folder">
          <span className="muted">{t("engines.folder")}</span>
          <span className="folder-path" title={folder}>
            {tilde(folder)}
          </span>
          {free !== null && (
            <span className="muted">
              {t("engines.free", { size: formatBytes(free) })}
            </span>
          )}
          <button onClick={() => void chooseFolder()}>
            {t("engines.change")}
          </button>
        </div>
      </header>

      <aside className="engine-projects" aria-label={t("engines.recent")}>
        <h2>{t("engines.recent")}</h2>
        {recent.length === 0 && (
          <p className="muted">{t("engines.recent_empty")}</p>
        )}
        {recent.map((p) => (
          <div key={p.path} className="project-card">
            <div className="split">
              <span className="project-name" title={p.path}>
                {p.name}
              </span>
              <span className="muted">
                {p.engine ? `UE ${p.engine}` : t("common.unknown_version")}
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
          <h2>{t("engines.on_disk")}</h2>
          {local.length === 0 ? (
            <p className="muted">{t("engines.none")}</p>
          ) : (
            <table className="installed">
              <thead>
                <tr>
                  <th scope="col">{t("engines.version")}</th>
                  <th scope="col">{t("engines.folder")}</th>
                  <th scope="col">{t("engines.origin")}</th>
                  <th scope="col">
                    <span className="sr-only">{t("engines.actions")}</span>
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
                          {e.byUehub ? "uehub" : t("engines.not_uehub")}
                        </span>
                      </td>
                      <td className="actions">
                        {removing === e.dir ? (
                          <span className="muted">{t("engines.deleting")}</span>
                        ) : confirming === e.dir ? (
                          <>
                            <span>{t("engines.confirm_delete")}</span>
                            <button
                              className="danger"
                              onClick={() => void uninstall(e)}
                            >
                              {t("common.delete")}
                            </button>
                            <button
                              className="quiet"
                              onClick={() => setConfirming(null)}
                            >
                              {t("common.cancel")}
                            </button>
                          </>
                        ) : (
                          <>
                            <button onClick={() => launch(e)}>
                              {t("common.launch")}
                            </button>
                            {e.byUehub && (
                              <button
                                className="quiet"
                                onClick={() => setConfirming(e.dir)}
                              >
                                {t("engines.uninstall")}
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
            <h2>{t("engines.published")}</h2>
            {catalog?.eulaAccepted && (
              <>
                <MatrixLegend />
                <button
                  className="quiet"
                  disabled={refreshing}
                  onClick={() => void engines.refresh()}
                >
                  {refreshing ? t("common.reloading") : t("common.reload")}
                </button>
              </>
            )}
          </div>
          {notice && (
            <p className="error" role="alert">
              {notice}
            </p>
          )}
          {catalogError && !catalog?.eulaAccepted ? (
            <div className="message">
              <p className="error">
                {t("engines.list_failed")} {catalogError}
              </p>
              <div className="buttons">
                <button onClick={() => void engines.refresh()}>
                  {t("common.retry")}
                </button>
                <LogButton />
              </div>
            </div>
          ) : !catalog ? (
            <p className="muted">{t("engines.connecting")}</p>
          ) : !catalog.eulaAccepted ? (
            <div className="message">
              <p>{t("engines.eula")}</p>
              <div className="buttons">
                <button
                  className="primary"
                  onClick={() => void api.openEulaPage()}
                >
                  {t("engines.read_eula")}
                </button>
                <button
                  disabled={refreshing}
                  onClick={() => void engines.refresh()}
                >
                  {t("engines.eula_accepted")}
                </button>
              </div>
            </div>
          ) : (
            <>
              {/* Epic ne répond pas : la liste gardée reste utilisable. */}
              {catalogError && (
                <p className="error" role="status">
                  {t("engines.stale", {
                    date: formatSeconds(catalog.checkedAt),
                  })}{" "}
                  {catalogError}
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
        {releaseText(update, date)}
        {jobFor(jobs, update) ? (
          ` ${t("engines.in_queue")}`
        ) : (
          <button className="primary" onClick={() => onInstall(update)}>
            {t("common.install_version", { version: update })}
          </button>
        )}
      </p>
    );
  }
  if (!replacedBy) return null;
  return (
    <p className="engine-note muted">
      {t("engines.replaced", {
        version: replacedBy,
        branch: branchOf(replacedBy),
      })}
      {engine.byUehub && ` ${t("engines.can_uninstall")}`}
    </p>
  );
}

const JOB_STATE = {
  failed: "engines.job_failed",
  paused: "engines.job_paused",
  waiting: "engines.job_waiting",
} as const;

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
    return (
      <button onClick={onOpen}>
        {t("engines.open_with", { version: project.opensWith })}
      </button>
    );
  if (!project.engine) return null;
  // La liste d'Epic est triée du plus récent au plus ancien : la première est la bonne.
  const candidate = available.find((e) =>
    e.version.startsWith(`${project.engine}.`),
  );
  const none = (
    <p className="muted small">
      {t("engines.no_engine_for", { engine: project.engine })}
    </p>
  );
  if (!candidate) return none;
  const job = jobFor(jobs, candidate.version);
  if (job) {
    const state = job.status.state;
    return (
      <p className="muted small">
        {t("engines.job_line", {
          version: candidate.version,
          state:
            state === "running"
              ? progressText(job.progress)
              : t(JOB_STATE[state === "done" ? "waiting" : state]),
        })}
      </p>
    );
  }
  return (
    <>
      {none}
      <button className="primary" onClick={() => onInstall(candidate.version)}>
        {t("common.install_version", { version: candidate.version })}
      </button>
    </>
  );
}
