// La mise à jour de uehub : la version installée, celle qui est publiée, et son installation.
import { useEffect, useRef, useState } from "react";
import {
    api,
    formatBytes,
    type AppInfo,
    type UpdateInfo,
    type UpdateProgress,
} from "./api";
import { RECHECK_MS, useTick } from "./recheck";

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

    // La dernière recherche, silencieuse ou à la main
    const lastCheck = useRef(0);

    // Une recherche silencieuse : hors ligne, on n'affiche rien et on garde ce qu'on sait.
    async function quietCheck() {
        lastCheck.current = Date.now();
        try {
            setAvailable(await api.checkUpdate());
        } catch {
            // Hors ligne : on réessaiera dans quatre heures.
        }
    }

    // Au démarrage, une première recherche.
    useEffect(() => {
        api.appInfo().then((info) => {
            setApp(info);
            if (info.package) void quietCheck();
        });
    }, []);

    // Puis une toutes les quatre heures tant que l'app reste ouverte, sauf pendant
    // l'installation d'une mise à jour.
    useTick(() => {
        if (!app?.package || progress) return;
        if (Date.now() - lastCheck.current < RECHECK_MS) return;
        void quietCheck();
    });

    async function check() {
        lastCheck.current = Date.now();
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