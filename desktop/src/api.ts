// Les appels vers Rust, typés. Chaque fonction correspond à une commande de commands.rs.
import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";

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

export interface Progress {
    phase: "chunks" | "files";
    done: number;
    total: number;
}

export interface RecentProject {
    path: string;
    name: string;
    engine: string | null;
}

type OnProgress = (p: Progress) => void;

// Un Channel transporte les messages de progression de Rust vers React.
function progressChannel(onProgress: OnProgress) {
    const channel = new Channel<Progress>();
    channel.onmessage = onProgress;
    return channel;
}

export const api = {
    session: () => invoke<Account | null>("session"),
    openLoginPage: () => invoke<void>("open_login_page"),
    login: (code: string) => invoke<Account>("login", { code }),
    logout: () => invoke<void>("logout"),
    library: () => invoke<Asset[]>("library"),
    download: (assetId: string, folder: string, onProgress: OnProgress) =>
        invoke<Summary>("download", {
            assetId,
            folder,
            onProgress: progressChannel(onProgress),
        }),
    addToProject: (
        assetId: string,
        uproject: string,
        overwrite: boolean,
        onProgress: OnProgress,
    ) =>
        invoke<Summary>("add_to_project", {
            assetId,
            uproject,
            overwrite,
            onProgress: progressChannel(onProgress),
        }),
    cancelJob: () => invoke<void>("cancel_job"),
    recentProjects: () => invoke<RecentProject[]>("recent_projects"),
    forgetProject: (uproject: string) =>
        invoke<void>("forget_project", { uproject }),
};

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