// La mise à jour de uehub : la version installée, celle qui est publiée, et son installation.
import { useEffect, useState } from "react";
import {
    api,
    formatBytes,
    type AppInfo,
    type UpdateInfo,
    type UpdateProgress,
} from "./api";

export interface AppUpdate {
    app: AppInfo | null;
    // La version publiée, plus récente que celle-ci
    available: UpdateInfo | null;
    checking: boolean;
    // Pendant l'installation ; uehub redémarre à la fin
    progress: UpdateProgress | null;
    // Le résultat d'une recherche à la main
    message: string | null;
    // Pourquoi l'installation a échoué
    error: string | null;
    check: () => Promise<void>;
    install: () => Promise<void>;
}

export function useAppUpdate(): AppUpdate {
    const [app, setApp] = useState<AppInfo | null>(null);
    const [available, setAvailable] = useState<UpdateInfo | null>(null);
    const [checking, setChecking] = useState(false);
    const [progress, setProgress] = useState<UpdateProgress | null>(null);
    const [message, setMessage] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    // Au démarrage, une recherche silencieuse : hors ligne, on n'affiche rien.
    useEffect(() => {
        api.appInfo().then((info) => {
            setApp(info);
            if (info.package) api.checkUpdate().then(setAvailable, () => {});
        });
    }, []);

    async function check() {
        setChecking(true);
        setMessage(null);
        try {
            const found = await api.checkUpdate();
            setAvailable(found);
            setMessage(
                found ? `uehub ${found.version} est disponible.` : "uehub est à jour.",
            );
        } catch (e) {
            setMessage(`Impossible de chercher une mise à jour : ${e}`);
        }
        setChecking(false);
    }

    async function install() {
        setError(null);
        setProgress({ step: "download", done: 0, total: null });
        try {
            await api.installUpdate(setProgress);
        } catch (e) {
            setProgress(null);
            setError(`La mise à jour a échoué : ${e}`);
        }
    }

    return { app, available, checking, progress, message, error, check, install };
}

// "Téléchargement : 12,4 Mo sur 31 Mo", puis "Installation…"
export function progressLabel(progress: UpdateProgress): string {
    if (progress.step === "install") return "Installation…";
    const done = formatBytes(progress.done);
    return progress.total
        ? `Téléchargement : ${done} sur ${formatBytes(progress.total)}`
        : `Téléchargement : ${done}`;
}