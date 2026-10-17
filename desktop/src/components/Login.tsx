import { useState, type FormEvent } from "react";

interface Props {
    onOpenPage: () => Promise<void>;
    onSubmit: (code: string) => Promise<void>;
}

// L'écran de connexion, dessiné comme un nœud de Blueprint posé sur la grille.
export default function Login({ onOpenPage, onSubmit }: Props) {
    const [code, setCode] = useState("");
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);

    async function openPage() {
        try {
            await onOpenPage();
        } catch (e) {
            setError(String(e));
        }
    }

    async function submit(event: FormEvent) {
        event.preventDefault();
        if (!code.trim()) return;
        setBusy(true);
        setError(null);
        try {
            await onSubmit(code.trim());
        } catch (e) {
            setError(String(e));
            setBusy(false);
        }
    }

    return (
        <main className="login">
            <form className="node" onSubmit={submit}>
                <header className="node-head">
                    <span className="exec" aria-hidden="true" />
                    <h1>Connexion à Epic Games</h1>
                </header>
                <div className="node-body">
                    <p className="muted">
                        Tu te connectes sur le site d'Epic : uehub ne voit jamais ton mot de
                        passe.
                    </p>
                    <ol className="steps">
                        <li>
                            Connecte-toi sur la page Epic.
                            <br />
                            <button type="button" onClick={openPage}>
                                Ouvrir la page de connexion
                            </button>
                        </li>
                        <li>
                            <label htmlFor="code">
                                Colle la valeur de <code>authorizationCode</code> affichée
                                ensuite.
                            </label>
                            <div className="pin-input">
                                <span className="pin pin-string" aria-hidden="true" />
                                <input
                                    id="code"
                                    value={code}
                                    onChange={(e) => setCode(e.target.value)}
                                    placeholder="authorizationCode"
                                    autoComplete="off"
                                    spellCheck={false}
                                />
                            </div>
                        </li>
                    </ol>
                    {error && (
                        <p className="error" role="alert">
                            {error}
                        </p>
                    )}
                    <button
                        className="primary"
                        type="submit"
                        disabled={busy || !code.trim()}
                    >
                        {busy ? "Connexion…" : "Se connecter"}
                    </button>
                </div>
            </form>
        </main>
    );
}