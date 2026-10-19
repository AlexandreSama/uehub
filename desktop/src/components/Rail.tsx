// Le rail à gauche : les deux écrans, et tout en bas la mise à jour et le compte.
import { useEffect, useRef, useState } from "react";
import { formatDay, tilde, type Account } from "../api";
import { progressLabel, type AppUpdate } from "../update";
import { EngineIcon, LibraryIcon, UpdateIcon } from "./Common";

// L'écran affiché à droite du rail : la bibliothèque Fab ou les moteurs.
export type View = "library" | "engines";

interface Props {
    view: View;
    onView: (view: View) => void;
    account: Account | null;
    onLogout: () => void;
    update: AppUpdate;
    // Des téléchargements tournent : le redémarrage les arrêtera
    queueBusy: boolean;
}

export default function Rail(props: Props) {
    const { view, onView, account, onLogout, update, queueBusy } = props;
    return (
        <nav className="rail" aria-label="Écrans">
            <span className="wordmark">uehub</span>
            <button
                className="rail-button"
                aria-current={view === "library" ? "page" : undefined}
                onClick={() => onView("library")}
            >
                <LibraryIcon />
                Bibliothèque
            </button>
            <button
                className="rail-button"
                aria-current={view === "engines" ? "page" : undefined}
                onClick={() => onView("engines")}
            >
                <EngineIcon />
                Moteurs
            </button>
            <div className="rail-foot">
                {update.available && (
                    <UpdateMenu update={update} queueBusy={queueBusy} />
                )}
                {account && (
                    <AccountMenu account={account} onLogout={onLogout} update={update} />
                )}
            </div>
        </nav>
    );
}

// Un menu qui s'ouvre au clic, et se referme sur un clic ailleurs ou sur Échap.
function usePopover() {
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!open) return;
        const outside = (e: PointerEvent) => {
            if (!box.current?.contains(e.target as Node)) setOpen(false);
        };
        const escape = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
        document.addEventListener("pointerdown", outside);
        document.addEventListener("keydown", escape);
        return () => {
            document.removeEventListener("pointerdown", outside);
            document.removeEventListener("keydown", escape);
        };
    }, [open]);

    return { open, setOpen, box };
}

// Le bouton ambre qui annonce une nouvelle version ; un clic ouvre ce qu'elle apporte.
function UpdateMenu({
                        update,
                        queueBusy,
                    }: {
    update: AppUpdate;
    queueBusy: boolean;
}) {
    const { open, setOpen, box } = usePopover();
    const { app, available, progress, error, install } = update;
    if (!app || !available) return null;
    // Un .deb ou un .rpm s'installe avec les droits d'administrateur.
    const asksPassword = app.package === "deb" || app.package === "rpm";

    return (
        <div className="update" ref={box}>
            {open && (
                <div className="update-menu" id="update-menu">
                    <strong>uehub {available.version} est disponible</strong>
                    <span className="muted">
            {available.date && `Publiée le ${formatDay(available.date)}. `}
                        Tu as la {app.version}.
          </span>
                    {available.notes && <p className="notes">{available.notes}</p>}
                    {queueBusy && (
                        <p className="muted">
                            La file tourne : elle s'arrêtera au redémarrage et reviendra en
                            pause.
                        </p>
                    )}
                    {asksPassword && (
                        <p className="muted">
                            Ton mot de passe sera demandé pour installer le paquet.
                        </p>
                    )}
                    <button
                        className="primary"
                        disabled={progress !== null}
                        onClick={() => void install()}
                    >
                        {progress ? progressLabel(progress) : "Installer et redémarrer"}
                    </button>
                    {error && (
                        <p className="error" role="alert">
                            {error}
                        </p>
                    )}
                </div>
            )}
            <button
                className="rail-button update-button"
                aria-expanded={open}
                aria-controls="update-menu"
                onClick={() => setOpen(!open)}
            >
                <UpdateIcon />
                Mise à jour
            </button>
        </div>
    );
}

// L'initiale du compte ; un clic ouvre son nom, où est rangée la session, la déconnexion,
// et la version de uehub.
function AccountMenu({
                         account,
                         onLogout,
                         update,
                     }: {
    account: Account;
    onLogout: () => void;
    update: AppUpdate;
}) {
    const { open, setOpen, box } = usePopover();
    const { app, checking, message, check } = update;

    const name = account.name || "Compte Epic";
    return (
        <div className="account" ref={box}>
            {open && (
                <div className="account-menu" id="account-menu">
                    <strong>{name}</strong>
                    <span className="muted">
            Session rangée dans : {tilde(account.storage)}
          </span>
                    <button onClick={onLogout}>Se déconnecter</button>
                    {app && (
                        <div className="app-version">
                            <span>uehub {app.version}</span>
                            {app.dev ? (
                                <span className="muted">
                  Version de développement : pas de mise à jour automatique.
                </span>
                            ) : !app.package ? (
                                <span className="muted">
                  Mise à jour par ton gestionnaire de paquets.
                </span>
                            ) : (
                                <>
                                    <button disabled={checking} onClick={() => void check()}>
                                        {checking ? "Recherche…" : "Rechercher une mise à jour"}
                                    </button>
                                    {message && <span className="muted">{message}</span>}
                                </>
                            )}
                        </div>
                    )}
                </div>
            )}
            <button
                className="avatar"
                aria-label={`Compte : ${name}`}
                aria-expanded={open}
                aria-controls="account-menu"
                onClick={() => setOpen(!open)}
            >
                {name.charAt(0).toUpperCase()}
            </button>
        </div>
    );
}