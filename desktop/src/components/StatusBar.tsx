import type { Progress } from "../api";

export interface Job {
    title: string;
    progress: Progress | null;
    // L'annulation est demandée, on attend que Rust s'arrête.
    cancelling: boolean;
}

export interface Status {
    tone: "ok" | "error";
    text: string;
}

const PHASE: Record<Progress["phase"], string> = {
    chunks: "Morceaux téléchargés",
    files: "Fichiers écrits",
};

interface Props {
    job: Job | null;
    status: Status | null;
    onCancel: () => void;
}

export default function StatusBar({ job, status, onCancel }: Props) {
    if (!job)
        return (
            <footer className={`statusbar ${status?.tone ?? ""}`}>
                <span role="status">{status?.text}</span>
            </footer>
        );

    const p = job.progress;
    const ratio = p && p.total > 0 ? p.done / p.total : 0;
    return (
        <footer className="statusbar">
            <span className="job-title">{job.title}</span>
            <span className="muted">
        {p ? `${PHASE[p.phase]} ${p.done} / ${p.total}` : "Préparation…"}
      </span>
            <div
                className="bar"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={p?.total ?? 0}
                aria-valuenow={p?.done ?? 0}
            >
                <div style={{ width: `${ratio * 100}%` }} />
            </div>
            <button className="danger" disabled={job.cancelling} onClick={onCancel}>
                {job.cancelling ? "Annulation…" : "Annuler"}
            </button>
        </footer>
    );
}