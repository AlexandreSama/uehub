// Les appels vers Rust, typés. Chaque fonction correspond à une commande de commands.rs.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type AssetKind = "pack" | "plugin" | "project";

export interface Asset {
    id: string;
    title: string;
    kind: AssetKind;
    category: string | null;
    thumbnail: string | null;
    engines: string[];
}

export interface Account {
    name: string;
    storage: string;
}

export interface Summary {
    files: number;
    bytes: number;
    dir: string;
    note: string | null;
}

export interface RecentProject {
    path: string;
    name: string;
    engine: string | null;
}

export interface Progress {
    phase: "chunks" | "files";
    done: number;
    total: number;
}

// Ce qu'on demande à la file (Request dans queue.rs).
export type JobRequest =
    | { kind: "download"; assetId: string; folder: string }
    | {
    kind: "addToProject";
    assetId: string;
    uproject: string;
    overwrite: boolean;
};

// Où en est un job (Status dans queue.rs).
export type JobStatus =
    | { state: "waiting" }
    | { state: "running" }
    | { state: "paused" }
    | { state: "done"; summary: Summary }
    | { state: "failed"; error: string };

export interface QueueJob {
    id: number;
    request: JobRequest;
    title: string;
    thumbnail: string | null;
    target: string;
    status: JobStatus;
    artifactId: string | null;
    finishedAt: number | null;
    progress: Progress | null;
}

export const api = {
    session: () => invoke<Account | null>("session"),
    openLoginPage: () => invoke<void>("open_login_page"),
    login: (code: string) => invoke<Account>("login", { code }),
    logout: () => invoke<void>("logout"),
    library: () => invoke<Asset[]>("library"),
    enqueue: (request: JobRequest) => invoke<void>("enqueue", { request }),
    jobs: () => invoke<QueueJob[]>("jobs"),
    pauseJob: (id: number) => invoke<void>("pause_job", { id }),
    resumeJob: (id: number) => invoke<void>("resume_job", { id }),
    removeJob: (id: number) => invoke<void>("remove_job", { id }),
    resumeAll: () => invoke<void>("resume_all"),
    clearFinished: () => invoke<void>("clear_finished"),
    recentProjects: () => invoke<RecentProject[]>("recent_projects"),
    forgetProject: (uproject: string) =>
        invoke<void>("forget_project", { uproject }),
};

// Rust envoie la file entière à chaque changement. Renvoie de quoi arrêter l'écoute.
export function onQueueChange(callback: (jobs: QueueJob[]) => void) {
    return listen<QueueJob[]>("queue", (event) => callback(event.payload));
}

// Les vignettes passent par le protocole thumb:// : Rust les garde en cache sur le disque.
export function thumbnailSrc(url: string): string {
    return convertFileSrc(url, "thumb");
}

export const KIND_LABEL: Record<AssetKind, string> = {
    pack: "Pack d'assets",
    plugin: "Plugin",
    project: "Projet complet",
};

// ["4.27", "5.0", "5.4"] → "UE 4.27 – 5.4"
export function engineRange(engines: string[]): string {
    if (engines.length === 0) return "Version inconnue";
    const first = engines[0];
    const last = engines[engines.length - 1];
    return first === last ? `UE ${first}` : `UE ${first} – ${last}`;
}

const number = new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 1 });

export function formatBytes(bytes: number): string {
    if (bytes >= 1e9) return `${number.format(bytes / 1e9)} Go`;
    if (bytes >= 1e6) return `${number.format(bytes / 1e6)} Mo`;
    return `${number.format(bytes / 1e3)} Ko`;
}

const PHASE: Record<Progress["phase"], string> = {
    chunks: "Morceaux téléchargés",
    files: "Fichiers écrits",
};

// "Morceaux téléchargés 33 / 112", ou "Préparation…" avant le premier message.
export function progressText(progress: Progress | null): string {
    return progress
        ? `${PHASE[progress.phase]} ${progress.done} / ${progress.total}`
        : "Préparation…";
}

// "61 fichiers, 1,2 Go"
export function summaryText(summary: Summary): string {
    return `${summary.files} fichiers, ${formatBytes(summary.bytes)}`;
}

// "Ajout à ForestShooter", "Téléchargement dans /home/…/Megascans_Grass"
export function jobTarget(job: QueueJob): string {
    return job.request.kind === "addToProject"
        ? `Ajout à ${job.target}`
        : `Téléchargement dans ${job.target}`;
}