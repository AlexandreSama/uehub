// Les appels vers Rust, typés. Chaque fonction correspond à une commande de commands.rs.
import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import { lang, t, type Key, type Lang } from "./i18n";

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

// Ce qu'a fait le retrait d'un asset (RemovalView dans commands.rs).
export interface Removal {
  deleted: number;
  // Gardés : un autre asset du projet les a aussi installés
  shared: number;
  // Déjà absents du projet
  missing: number;
}

// Un asset que uehub a ajouté à un projet récent (AddedView dans commands.rs).
export interface AddedAsset {
  // Le .uproject du projet
  project: string;
  assetId: string;
  // La version d'Unreal visée par la version installée, ex. "5.3"
  engine: string;
  // Date de l'ajout, en secondes depuis 1970
  addedAt: number;
  files: number;
  update: AssetUpdate | null;
}

// Pourquoi le mettre à jour : une nouvelle build de la même version, ou une version
// plus proche du moteur du projet.
export type AssetUpdate =
  | { kind: "newBuild" }
  | { kind: "closerEngine"; engine: string };

// L'avancement du job en cours (ProgressEvent dans commands.rs).
export type Progress =
  // Les morceaux d'un asset, et les octets reçus pour les télécharger
  | { phase: "chunks"; done: number; total: number; bytes: number }
  // Les fichiers écrits (asset) ou extraits (moteur)
  | { phase: "files"; done: number; total: number }
  // Le zip d'un moteur, en octets
  | { phase: "bytes"; done: number; total: number };

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
  // Taille du zip, puis place estimée une fois extrait, en octets
  size: number;
  extracted: number;
  date: string | null;
}

export interface EngineCatalog {
  eulaAccepted: boolean;
  // Quand la liste a été lue sur unrealengine.com, en secondes depuis 1970
  checkedAt: number;
  engines: AvailableEngine[];
}

// Cette copie de uehub (AppInfo dans update.rs).
export interface AppInfo {
  version: string;
  // Le paquet d'où vient uehub : il se met à jour seul. null : par le gestionnaire de paquets
  package: "deb" | "rpm" | "appimage" | null;
  // Lancée par `tauri dev`
  dev: boolean;
}

// Une version plus récente, publiée sur GitHub Releases (UpdateInfo dans update.rs).
export interface UpdateInfo {
  version: string;
  date: string | null;
  notes: string | null;
}

// L'avancement de l'installation : le téléchargement, puis l'installation du paquet.
export type UpdateProgress =
  | { step: "download"; done: number; total: number | null }
  | { step: "install" };

// Un moteur enregistré dans Install.ini.
export interface LocalEngine {
  version: string | null;
  dir: string;
  // Installé par uehub : on peut le désinstaller d'ici
  byUehub: boolean;
  // La version plus récente de sa branche chez Epic : "5.8.4" pour une 5.8.3
  update: string | null;
  // Un moteur plus récent de sa branche est installé : il ouvre les projets à sa place
  replacedBy: string | null;
}

export const api = {
  session: () => invoke<Account | null>("session"),
  openLoginPage: () => invoke<void>("open_login_page"),
  // La page Fab de l'asset ; sans asset, ta bibliothèque sur Fab.
  openFab: (assetId: string | null) => invoke<void>("open_fab", { assetId }),
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
  addedAssets: () => invoke<AddedAsset[]>("added_assets"),
  removeAsset: (uproject: string, assetId: string) =>
    invoke<Removal>("remove_asset", { uproject, assetId }),
  localEngines: () => invoke<LocalEngine[]>("local_engines"),
  cachedEngines: () => invoke<EngineCatalog | null>("cached_engines"),
  availableEngines: () => invoke<EngineCatalog>("available_engines"),
  enginesFolder: () => invoke<string>("engines_folder"),
  setEnginesFolder: (folder: string) =>
    invoke<void>("set_engines_folder", { folder }),
  freeSpace: (folder: string) => invoke<number>("free_space", { folder }),
  openEulaPage: () => invoke<void>("open_eula_page"),
  launchEngine: (dir: string) => invoke<void>("launch_engine", { dir }),
  openProject: (uproject: string) => invoke<void>("open_project", { uproject }),
  removeEngine: (dir: string) => invoke<void>("remove_engine", { dir }),
  appInfo: () => invoke<AppInfo>("app_info"),
  // La langue choisie par Rust d'après celle du système
  language: () => invoke<Lang>("language"),
  // Le chemin du journal, et de quoi le montrer dans le gestionnaire de fichiers
  logFile: () => invoke<string | null>("log_file"),
  showLog: () => invoke<void>("show_log"),
  // Une erreur JavaScript, écrite dans le journal
  logWebview: (message: string) => invoke<void>("log_webview", { message }),
  checkUpdate: () => invoke<UpdateInfo | null>("check_update"),
  // L'avancement arrive par un canal ; si tout va bien, uehub redémarre à la fin.
  installUpdate: (onProgress: (progress: UpdateProgress) => void) => {
    const channel = new Channel<UpdateProgress>();
    channel.onmessage = onProgress;
    return invoke<void>("install_update", { onProgress: channel });
  },
};

// Rust envoie la file entière à chaque changement. Renvoie de quoi arrêter l'écoute.
export function onQueueChange(callback: (jobs: QueueJob[]) => void) {
  return listen<QueueJob[]>("queue", (event) => callback(event.payload));
}

// Les vignettes passent par le protocole thumb:// : Rust les garde en cache sur le disque.
export function thumbnailSrc(url: string): string {
  return convertFileSrc(url, "thumb");
}

// Le dossier personnel, lu une fois au démarrage pour afficher les chemins avec « ~ ».
let home = "";

export async function loadHome() {
  try {
    home = (await homeDir()).replace(/\/$/, "");
  } catch {
    home = "";
  }
}

// "/home/alex/UnrealEngine/5.8.3" → "~/UnrealEngine/5.8.3"
export function tilde(path: string): string {
  if (home && (path === home || path.startsWith(`${home}/`)))
    return `~${path.slice(home.length)}`;
  return path;
}

const KIND: Record<AssetKind, Key> = {
  pack: "kinds.pack",
  plugin: "kinds.plugin",
  project: "kinds.project",
};

// "Pack d'assets", "Plugin", "Projet complet"
export function kindLabel(kind: AssetKind): string {
  return t(KIND[kind]);
}

// "5.10" après "5.9" : on compare les nombres, pas le texte.
export function compareVersions(a: string, b: string): number {
  const x = a.split(".").map(Number);
  const y = b.split(".").map(Number);
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

// "5.8.3" → "5.8"
export function branchOf(version: string): string {
  return version.split(".").slice(0, 2).join(".");
}

// ["4.27", "5.0", "5.4"] → "4.27 – 5.4"
export function engineRange(engines: string[]): string {
  if (engines.length === 0) return t("common.unknown_version");
  const first = engines[0];
  const last = engines[engines.length - 1];
  return first === last ? first : `${first} – ${last}`;
}

// 1 234 567 890 → "1,2 Go" en français, "1.2 GB" en anglais.
export function formatBytes(bytes: number): string {
  const number = (value: number) =>
    new Intl.NumberFormat(lang(), { maximumFractionDigits: 1 }).format(value);
  if (bytes >= 1e9) return t("units.gb", { value: number(bytes / 1e9) });
  if (bytes >= 1e6) return t("units.mb", { value: number(bytes / 1e6) });
  return t("units.kb", { value: number(bytes / 1e3) });
}

// 18 400 000 → "18,4 Mo/s"
export function formatRate(bytesPerSecond: number): string {
  return t("format.rate", { size: formatBytes(bytesPerSecond) });
}

// 1 500 → "25 min", 4 300 → "1 h 12 min"
export function formatDuration(seconds: number): string {
  if (seconds < 60) return t("format.under_a_minute");
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return t("format.minutes", { m: minutes });
  const rest = minutes % 60;
  const h = Math.floor(minutes / 60);
  return rest > 0
    ? t("format.hours_minutes", { h, m: rest })
    : t("format.hours", { h });
}

// La part faite, entre 0 et 1.
export function progressRatio(progress: Progress | null): number {
  return progress && progress.total > 0 ? progress.done / progress.total : 0;
}

const PHASE: Record<Progress["phase"], Key> = {
  chunks: "progress.chunks",
  files: "progress.files",
  bytes: "progress.bytes",
};

// "Morceaux téléchargés 33 / 112", "Téléchargé 12,3 Go / 39,8 Go",
// ou "Préparation…" avant le premier message.
export function progressText(progress: Progress | null): string {
  if (!progress) return t("progress.preparing");
  const { phase, done, total } = progress;
  return phase === "bytes"
    ? t(PHASE[phase], { done: formatBytes(done), total: formatBytes(total) })
    : t(PHASE[phase], { done, total });
}

// "61 fichiers, 1,2 Go", comme Summary::text côté Rust.
export function summaryText(summary: Summary): string {
  return t("summary.files", {
    n: summary.files,
    size: formatBytes(summary.bytes),
  });
}

const TARGET: Record<JobRequest["kind"], Key> = {
  download: "jobs.target.download",
  addToProject: "jobs.target.addToProject",
  installEngine: "jobs.target.installEngine",
};

// "Ajout à ForestShooter", "Téléchargement dans ~/Assets/Megascans_Grass"
export function jobTarget(job: QueueJob): string {
  return t(TARGET[job.request.kind], { target: tilde(job.target) });
}

const DONE_TARGET: Record<JobRequest["kind"], Key> = {
  download: "jobs.done.download",
  addToProject: "jobs.done.addToProject",
  installEngine: "jobs.done.installEngine",
};

// "Ajouté à ArchViz_Loft", "Installé dans ~/UnrealEngine/5.8.3"
export function doneTarget(job: QueueJob): string {
  return t(DONE_TARGET[job.request.kind], { target: tilde(job.target) });
}

// La version qu'installe un job, s'il installe un moteur et n'est pas terminé.
export function installing(job: QueueJob): string | null {
  return job.request.kind === "installEngine" && job.status.state !== "done"
    ? job.request.version
    : null;
}

// Le job de la file qui installe cette version, s'il n'est pas terminé.
export function jobFor(jobs: QueueJob[], version: string) {
  return jobs.find((j) => installing(j) === version);
}

// "2026-09-22" → "22 sept. 2026", "Sep 22, 2026"
export function formatDay(date: string | null): string {
  if (!date) return "";
  const day = new Intl.DateTimeFormat(lang(), {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
  return day.format(new Date(date));
}

// "La 5.8.4 est sortie le 14 oct. 2026." : la date seulement si Epic la donne.
export function releaseText(version: string, date: string | null | undefined) {
  return date
    ? t("engines.out_on", { version, date: formatDay(date) })
    : t("engines.out", { version });
}

// Une date notée par uehub, en secondes : le jour sur l'horloge de l'ordinateur.
export function formatSeconds(seconds: number): string {
  const day = new Intl.DateTimeFormat(lang(), {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
  return day.format(new Date(seconds * 1000));
}

// Ce qu'apporte une mise à jour, en une phrase.
export function updateText(update: AssetUpdate, installed: string): string {
  return update.kind === "newBuild"
    ? t("assets.new_build", { installed })
    : t("assets.closer_engine", { engine: update.engine, installed });
}

// "48 fichiers supprimés. 3 fichiers gardés : un autre asset du projet les utilise aussi."
export function removalText(removal: Removal): string {
  const { deleted, shared, missing } = removal;
  const parts = [t("removal.deleted", { n: deleted })];
  if (shared > 0) parts.push(t("removal.shared", { n: shared }));
  if (missing > 0) parts.push(t("removal.missing", { n: missing }));
  return parts.join(" ");
}
