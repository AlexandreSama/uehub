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
    // La version du moteur installé qui l'ouvrira, ex. "5.4.4"
    opensWith: string | null;
}

export interface Progress {
    // "bytes" : le zip d'un moteur, en octets
    phase: "chunks" | "files" | "bytes";
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
}
    | { kind: "installEngine"; version: string; folder: string };

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

// Un moteur proposé par Epic (AvailableEngine dans commands.rs).
export interface AvailableEngine {
    version: string;
    size: number;
    date: string | null;
}

export interface EngineCatalog {
    eulaAccepted: boolean;
    engines: AvailableEngine[];
}

// Un moteur enregistré dans Install.ini.
export interface LocalEngine {
    version: string | null;
    dir: string;
    // Installé par uehub : on peut le désinstaller d'ici
    byUehub: boolean;
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
    localEngines: () => invoke<LocalEngine[]>("local_engines"),
    availableEngines: () => invoke<EngineCatalog>("available_engines"),
    enginesFolder: () => invoke<string>("engines_folder"),
    setEnginesFolder: (folder: string) =>
        invoke<void>("set_engines_folder", { folder }),
    openEulaPage: () => invoke<void>("open_eula_page"),
    launchEngine: (dir: string) => invoke<void>("launch_engine", { dir }),
    openProject: (uproject: string) => invoke<void>("open_project", { uproject }),
    removeEngine: (dir: string) => invoke<void>("remove_engine", { dir }),
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
    bytes: "Téléchargé",
};

// "Morceaux téléchargés 33 / 112", "Téléchargé 12,3 Go / 39,8 Go",
// ou "Préparation…" avant le premier message.
export function progressText(progress: Progress | null): string {
    if (!progress) return "Préparation…";
    const { phase, done, total } = progress;
    return phase === "bytes"
        ? `${PHASE[phase]} ${formatBytes(done)} / ${formatBytes(total)}`
        : `${PHASE[phase]} ${done} / ${total}`;
}

// "61 fichiers, 1,2 Go"
export function summaryText(summary: Summary): string {
    return `${summary.files} fichiers, ${formatBytes(summary.bytes)}`;
}

const TARGET: Record<JobRequest["kind"], string> = {
    download: "Téléchargement dans",
    addToProject: "Ajout à",
    installEngine: "Installation dans",
};

// "Ajout à ForestShooter", "Téléchargement dans /home/…/Megascans_Grass"
export function jobTarget(job: QueueJob): string {
    return `${TARGET[job.request.kind]} ${job.target}`;
}

// Le verbe du bilan : "Ajouté : Megascans: Grass, 61 fichiers, 1,2 Go"
export const DONE: Record<JobRequest["kind"], string> = {
    download: "Téléchargé",
    addToProject: "Ajouté",
    installEngine: "Installé",
};