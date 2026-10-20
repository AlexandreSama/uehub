import {
  formatRate,
  jobTarget,
  progressText,
  summaryText,
  type QueueJob,
} from "../api";
import { t } from "../i18n";
import type { Rate } from "../rates";
import { EngineMark, ProgressBar, Thumb } from "./Common";
import LogButton from "./LogButton";

interface Props {
  jobs: QueueJob[];
  rates: Record<number, Rate>;
  onPause: (id: number) => void;
  onResume: (id: number) => void;
  onRemove: (id: number) => void;
  onResumeAll: () => void;
  onClearFinished: () => void;
}

// Le panneau de la file : un job par ligne, dans l'ordre où ils seront traités.
export default function QueueDrawer(props: Props) {
  const { jobs, rates, onPause, onResume, onRemove } = props;
  const anyPaused = jobs.some((j) => j.status.state === "paused");
  const anyDone = jobs.some((j) => j.status.state === "done");
  const anyFailed = jobs.some((j) => j.status.state === "failed");

  return (
    <section id="queue" className="queue" aria-label={t("queue.title")}>
      <header className="queue-head">
        <h2>{t("queue.title")}</h2>
        {anyFailed && <LogButton />}
        {anyPaused && (
          <button onClick={props.onResumeAll}>{t("queue.resume_all")}</button>
        )}
        {anyDone && (
          <button className="quiet" onClick={props.onClearFinished}>
            {t("queue.clear_done")}
          </button>
        )}
      </header>

      {jobs.length === 0 ? (
        <p className="muted">{t("queue.drawer_empty")}</p>
      ) : (
        <ol className="queue-list">
          {jobs.map((job) => {
            const state = job.status.state;
            return (
              <li key={job.id} className={`queue-item ${state}`}>
                {job.request.kind === "installEngine" ? (
                  <EngineMark version={job.request.version} />
                ) : (
                  <Thumb url={job.thumbnail} />
                )}
                <div className="queue-what">
                  <span className="queue-title">{job.title}</span>
                  <span className="muted" title={job.target}>
                    {jobTarget(job)}
                  </span>
                </div>
                <JobState job={job} rate={rates[job.id]} />
                <div className="queue-actions">
                  {(state === "running" || state === "waiting") && (
                    <button className="quiet" onClick={() => onPause(job.id)}>
                      {t("common.pause")}
                    </button>
                  )}
                  {(state === "paused" || state === "failed") && (
                    <button className="quiet" onClick={() => onResume(job.id)}>
                      {state === "paused"
                        ? t("common.resume")
                        : t("common.retry")}
                    </button>
                  )}
                  <button
                    className="quiet"
                    aria-label={t("queue.remove_job", { title: job.title })}
                    onClick={() => onRemove(job.id)}
                  >
                    {t("queue.remove")}
                  </button>
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </section>
  );
}

function JobState({ job, rate }: { job: QueueJob; rate?: Rate }) {
  const status = job.status;
  switch (status.state) {
    case "running":
      return (
        <div className="queue-state">
          {progressText(job.progress)}
          {rate?.speed != null && `, ${formatRate(rate.speed)}`}
          <ProgressBar progress={job.progress} />
        </div>
      );
    case "waiting":
      return <div className="queue-state muted">{t("common.waiting")}</div>;
    case "paused":
      return <div className="queue-state muted">{t("common.paused")}</div>;
    case "done":
      return (
        <div className="queue-state" title={status.summary.note ?? undefined}>
          {t("queue.done", { summary: summaryText(status.summary) })}
        </div>
      );
    case "failed":
      return (
        <div className="queue-state error" title={status.error}>
          {status.error}
        </div>
      );
  }
}
