// La file en bas de l'écran, comme une piste du séquenceur : le dernier job terminé,
// celui en cours (rempli au fil de sa progression), puis ce qui attend : en attente,
// en pause ou en échec.
import {
    doneTarget,
    formatDuration,
    formatRate,
    jobTarget,
    progressRatio,
    progressText,
    type QueueJob,
} from "../api";
import type { Rate } from "../rates";

interface Props {
    jobs: QueueJob[];
    rates: Record<number, Rate>;
    open: boolean;
    onToggle: () => void;
    onPause: (id: number) => void;
    onResumeAll: () => void;
}

// Au-delà de deux jobs qui attendent, un simple « +3 ».
const SHOWN = 2;

export default function QueueDock(props: Props) {
    const { jobs, rates, open, onToggle, onPause, onResumeAll } = props;
    const running = jobs.find((j) => j.status.state === "running");
    // Un job en échec attend lui aussi : qu'on le réessaie ou qu'on le retire.
    const waiting = jobs.filter((j) =>
        ["waiting", "paused", "failed"].includes(j.status.state),
    );
    const anyPaused = waiting.some((j) => j.status.state === "paused");
    // Le dernier terminé dans le temps, pas dans l'ordre de la file.
    const last = jobs
        .filter((j) => j.status.state === "done")
        .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0))[0];
    const pending = waiting.length + (running ? 1 : 0);

    return (
        <footer className="dock" aria-label="File d'attente">
            <h2>File</h2>
            <ol className="track">
                {jobs.length === 0 && (
                    <li className="clip empty">
                        La file est vide. Ajoute un asset à un projet ou installe un moteur
                        : il apparaîtra ici.
                    </li>
                )}
                {last && <Clip job={last} line={doneTarget(last)} className="muted" />}
                {running && <RunningClip job={running} rate={rates[running.id]} />}
                {waiting
                    .slice(0, SHOWN)
                    .map((job) =>
                        job.status.state === "failed" ? (
                            <Clip
                                key={job.id}
                                job={job}
                                line={`Échec : ${job.status.error}`}
                                className="error"
                            />
                        ) : (
                            <Clip
                                key={job.id}
                                job={job}
                                line={
                                    job.status.state === "paused" ? "En pause" : jobTarget(job)
                                }
                                className="muted"
                            />
                        ),
                    )}
                {waiting.length > SHOWN && (
                    <li className="clip more">+{waiting.length - SHOWN}</li>
                )}
            </ol>
            {running ? (
                <button onClick={() => onPause(running.id)}>Pause</button>
            ) : (
                anyPaused && <button onClick={onResumeAll}>Tout reprendre</button>
            )}
            <button
                className="quiet"
                aria-expanded={open}
                aria-controls="queue"
                onClick={onToggle}
            >
                {open
                    ? "Masquer"
                    : pending > 0
                        ? `Tout voir (${pending})`
                        : "Tout voir"}
            </button>
        </footer>
    );
}

// Le job en cours : le fond se remplit, la vitesse et le temps restant à droite.
function RunningClip({ job, rate }: { job: QueueJob; rate?: Rate }) {
    return (
        <li className="clip running">
      <span
          className="clip-fill"
          style={{ width: `${progressRatio(job.progress) * 100}%` }}
      />
            <span className="clip-line">
        <span className="clip-title">{job.title}</span>
                {rate?.speed != null && (
                    <span className="clip-rate">{formatRate(rate.speed)}</span>
                )}
      </span>
            <span className="clip-line">
        <span>{progressText(job.progress)}</span>
                {rate?.remaining != null && (
                    <span>encore {formatDuration(rate.remaining)}</span>
                )}
      </span>
        </li>
    );
}

// Un job qui n'avance pas : son titre, et une ligne sur son état.
function Clip(props: { job: QueueJob; line: string; className: string }) {
    const { job, line, className } = props;
    return (
        <li className={`clip ${job.status.state}`} title={line}>
      <span className="clip-line">
        <span className="clip-title">{job.title}</span>
      </span>
            <span className={`clip-line ${className}`}>
        <span>{line}</span>
      </span>
        </li>
    );
}