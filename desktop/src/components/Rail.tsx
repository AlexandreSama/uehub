// Le rail à gauche : les deux écrans, et le compte tout en bas.
import { useEffect, useRef, useState } from "react";
import { tilde, type Account } from "../api";
import { EngineIcon, LibraryIcon } from "./Common";

// L'écran affiché à droite du rail : la bibliothèque Fab ou les moteurs.
export type View = "library" | "engines";

interface Props {
    view: View;
    onView: (view: View) => void;
    account: Account | null;
    onLogout: () => void;
}

export default function Rail({ view, onView, account, onLogout }: Props) {
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
            {account && <AccountMenu account={account} onLogout={onLogout} />}
        </nav>
    );
}

// L'initiale du compte ; un clic ouvre son nom, où est rangée la session, et la déconnexion.
function AccountMenu({
                         account,
                         onLogout,
                     }: {
    account: Account;
    onLogout: () => void;
}) {
    const [open, setOpen] = useState(false);
    const box = useRef<HTMLDivElement>(null);

    // Un clic ailleurs, ou Échap, referme le menu.
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