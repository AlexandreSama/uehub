// L'inspecteur de l'écran Moteurs : la version choisie dans la grille. Installée, on
// la lance, et on voit si une plus récente de sa branche est sortie ; dans la file, on
// suit son avancement et sa vitesse ; sinon, on voit la place qu'elle prendra avant de
// l'installer.
import {
  branchOf,
  formatBytes,
  formatDay,
  formatDuration,
  formatRate,
  progressRatio,
  progressText,
  releaseText,
  tilde,
  type AvailableEngine,
  type LocalEngine,
  type QueueJob,
} from "../api";
import { t } from "../i18n";
import type { Rate } from "../rates";
import { ProgressBar, SpeedGraph } from "./Common";
import LogButton from "./LogButton";

interface Props {
  engine: AvailableEngine | undefined;
  installed: LocalEngine | undefined;
  // La version plus récente de la même branche, si celle-ci est installée et en a une
  newer: AvailableEngine | undefined;
  // Son installation dans la file, s'il y en a une
  newerJob: QueueJob | undefined;
  job: QueueJob | undefined;
  rate: Rate | undefined;
  folder: string;
  // Place libre dans le dossier des moteurs, `null` tant qu'on ne la connaît pas
  free: number | null;
  onInstall: (version: string) => void;
  // Choisir une autre version dans la grille
  onSelect: (version: string) => void;
  onLaunch: (engine: LocalEngine) => void;
  onPause: (id: number) => void;
  onResume: (id: number) => void;
  onRemove: (id: number) => void;
}

export default function EngineInspector(props: Props) {
  const { engine, installed, job } = props;
  if (!engine)
    return <aside className="inspector empty">{t("inspector.empty")}</aside>;

  return (
    <aside className="inspector" aria-label={t("inspector.title")}>
      <div>
        <h2>Unreal Engine {engine.version}</h2>
        {engine.date && (
          <p className="muted">
            {t("common.published_on", { date: formatDay(engine.date) })}
          </p>
        )}
      </div>
      {installed ? (
        <>
          <section className="block">
            <h3>{t("engines.on_disk")}</h3>
            <p className="path" title={installed.dir}>
              {tilde(installed.dir)}
            </p>
            <div className="buttons">
              <button
                className={installed.update ? undefined : "primary"}
                onClick={() => props.onLaunch(installed)}
              >
                {t("common.launch")}
              </button>
            </div>
            {installed.replacedBy && (
              <p className="muted">
                {t("inspector.replaced", {
                  version: installed.replacedBy,
                  branch: branchOf(installed.replacedBy),
                })}
                {installed.byUehub && ` ${t("inspector.can_uninstall")}`}
              </p>
            )}
          </section>
          {installed.update && (
            <UpdateBlock
              {...props}
              installed={installed}
              version={installed.update}
            />
          )}
        </>
      ) : job ? (
        <JobBlock {...props} job={job} />
      ) : (
        <InstallBlock {...props} engine={engine} />
      )}
    </aside>
  );
}

// Epic a publié une version plus récente de la même branche : elle s'installe à côté.
function UpdateBlock(
  props: Props & { installed: LocalEngine; version: string },
) {
  const { installed, version, newer, newerJob, onInstall, onSelect } = props;
  return (
    <section className="block">
      <h3>{t("common.update")}</h3>
      <p>
        {releaseText(version, newer?.date)}{" "}
        {t("inspector.update_next", {
          installed: installed.version ?? "?",
          branch: branchOf(version),
        })}
      </p>
      <div className="buttons">
        {newerJob ? (
          <button onClick={() => onSelect(version)}>
            {t("inspector.follow")}
          </button>
        ) : (
          <>
            <button className="primary" onClick={() => onInstall(version)}>
              {t("common.install_version", { version })}
            </button>
            <button className="quiet" onClick={() => onSelect(version)}>
              {t("inspector.see", { version })}
            </button>
          </>
        )}
      </div>
    </section>
  );
}

// Une installation dans la file : en cours, en attente, en pause ou en échec.
function JobBlock(props: Props & { job: QueueJob }) {
  const { job, rate, onPause, onResume, onRemove } = props;
  const remove = (
    <button className="quiet" onClick={() => onRemove(job.id)}>
      {t("inspector.remove_from_queue")}
    </button>
  );

  if (job.status.state === "running") {
    const extracting = job.progress?.phase === "files";
    return (
      <section className="block">
        <div className="split">
          <strong>
            {extracting
              ? t("inspector.extracting")
              : t("inspector.downloading")}
          </strong>
          <span className="num">
            {Math.floor(progressRatio(job.progress) * 100)} %
          </span>
        </div>
        <ProgressBar progress={job.progress} />
        <div className="split muted">
          <span>{progressText(job.progress)}</span>
          {rate?.remaining != null && (
            <span>
              {t("common.time_left", { time: formatDuration(rate.remaining) })}
            </span>
          )}
        </div>
        {rate?.speed != null && <Speed rate={rate} speed={rate.speed} />}
        <div className="buttons">
          <button onClick={() => onPause(job.id)}>{t("common.pause")}</button>
          {remove}
        </div>
      </section>
    );
  }

  const failed = job.status.state === "failed" ? job.status.error : null;
  return (
    <section className="block">
      {failed ? (
        <p className="error">{failed}</p>
      ) : (
        <p>
          {job.status.state === "paused"
            ? t("inspector.paused")
            : t("inspector.waiting")}
        </p>
      )}
      <div className="buttons">
        {job.status.state === "waiting" ? (
          <button onClick={() => onPause(job.id)}>{t("common.pause")}</button>
        ) : (
          <button onClick={() => onResume(job.id)}>
            {failed ? t("common.retry") : t("common.resume")}
          </button>
        )}
        {remove}
        {failed && <LogButton />}
      </div>
    </section>
  );
}

// La vitesse maintenant, et celle de la dernière minute en barres.
function Speed({ rate, speed }: { rate: Rate; speed: number }) {
  const average =
    rate.history.length > 0
      ? rate.history.reduce((a, b) => a + b, 0) / rate.history.length
      : null;
  return (
    <div className="speed">
      <div className="split">
        <h3>{t("inspector.speed")}</h3>
        <span className="speed-now">{formatRate(speed)}</span>
      </div>
      <SpeedGraph history={rate.history} />
      <div className="split muted small">
        <span>{t("inspector.last_minute")}</span>
        {average !== null && (
          <span>{t("inspector.average", { rate: formatRate(average) })}</span>
        )}
      </div>
    </div>
  );
}

// Une version à installer : la place qu'elle prendra, puis où elle ira.
function InstallBlock(props: Props & { engine: AvailableEngine }) {
  const { engine, folder, free, onInstall } = props;
  const needed = engine.size + engine.extracted;
  // Les deux parts de la barre, en pourcentage de la place libre.
  const zip = free ? Math.min(engine.size / free, 1) : 0;
  const extracted = free ? Math.min(engine.extracted / free, 1 - zip) : 0;

  return (
    <>
      <section className="block">
        <h3>{t("inspector.disk")}</h3>
        {free !== null && (
          <div
            className="disk"
            role="img"
            aria-label={t("inspector.disk_label", {
              needed: formatBytes(needed),
              free: formatBytes(free),
            })}
          >
            <span className="disk-zip" style={{ width: `${zip * 100}%` }} />
            <span
              className="disk-engine"
              style={{ width: `${extracted * 100}%` }}
            />
          </div>
        )}
        <dl className="disk-legend">
          <div>
            <dt>
              <span className="swatch disk-zip" />
              {t("inspector.zip")}
            </dt>
            <dd>{formatBytes(engine.size)}</dd>
          </div>
          <div>
            <dt>
              <span className="swatch disk-engine" />
              {t("inspector.extracted")}
            </dt>
            <dd>
              {t("inspector.about", { size: formatBytes(engine.extracted) })}
            </dd>
          </div>
          {free !== null && (
            <div className="muted">
              <dt>
                <span className="swatch disk-free" />
                {t("inspector.free_after")}
              </dt>
              <dd>{formatBytes(Math.max(free - engine.extracted, 0))}</dd>
            </div>
          )}
        </dl>
        <p className="muted small">{t("inspector.zip_deleted")}</p>
        {free !== null && free < needed && (
          <p className="error">
            {t("inspector.no_space", {
              needed: formatBytes(needed),
              free: formatBytes(free),
            })}
          </p>
        )}
      </section>
      <section className="block">
        <h3>{t("inspector.destination")}</h3>
        <p className="path">
          {tilde(`${folder.replace(/\/$/, "")}/${engine.version}`)}
        </p>
        <div className="buttons">
          <button className="primary" onClick={() => onInstall(engine.version)}>
            {t("common.install_version", { version: engine.version })}
          </button>
        </div>
      </section>
    </>
  );
}
