import { progressText, summaryText, type QueueJob } from "../api";
import { ProgressBar } from "./Common";

interface Props {
    jobs: QueueJob[];
    open: boolean;
    onToggle: () => void;
    onResumeAll: () => void;
}

// La barre du bas : le job en cours, sinon ce qui attend une action, sinon le dernier bilan.
export default function QueueBar({ jobs, open, onToggle, onResumeAll }: Props) {
    const running = jobs.find((j) => j.status.state === "running");
    const waiting = jobs.filter((j) => j.status.state === "waiting").length;
    const paused = jobs.filter((j) => j.status.state === "paused").length;
    const pending = jobs.filter((j) => j.status.state !== "done").length;
    // Le bilan du dernier job terminé (dans le temps, pas dans l'ordre de la file).
    const last = jobs
        .filter((j) => j.status.state === "done" || j.status.state === "failed")
        .sort((a, b) => (b.finishedAt ?? 0) - (a.finishedAt ?? 0))[0];

    return (
        <footer className="statusbar">
            {running ? (
                <>
                    <span className="job-title">{running.title}</span>
                    <span className="muted">{progressText(running.progress)}</span>
                    <ProgressBar progress={running.progress} />
                    {waiting > 0 && <span className="muted">{waiting} en attente</span>}
                </>
            ) : paused > 0 ? (
                <>
          <span>
            {paused === 1
                ? "1 téléchargement en pause"
                : `${paused} téléchargements en pause`}
          </span>
                    <button onClick={onResumeAll}>Tout reprendre</button>
                </>
            ) : (
                last && <LastResult job={last} />
            )}
            <button
                className="quiet queue-toggle"
                aria-expanded={open}
                aria-controls="queue"
                onClick={onToggle}
            >
                {pending > 0 ? `File d'attente (${pending})` : "File d'attente"}
            </button>
        </footer>
    );
}

function LastResult({ job }: { job: QueueJob }) {
    if (job.status.state === "failed")
        return (
            <span className="error" title={job.status.error}>
        {job.title} : {job.status.error}
      </span>
        );
    if (job.status.state !== "done") return null;

    const verb = job.request.kind === "addToProject" ? "Ajouté" : "Téléchargé";
    const { summary } = job.status;
    const text = `${verb} : ${job.title}, ${summaryText(summary)}`;
    return <span>{summary.note ? `${text}. ${summary.note}` : text}</span>;
}