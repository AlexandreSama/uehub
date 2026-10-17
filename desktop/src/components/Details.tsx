import { KIND_LABEL, type Asset, type RecentProject } from "../api";
import { KindPin, Thumb } from "./Common";

interface Props {
    asset: Asset | null;
    recent: RecentProject[];
    busy: boolean;
    overwrite: boolean;
    onOverwrite: (value: boolean) => void;
    // `null` : ouvrir la boîte de dialogue pour choisir le projet
    onAdd: (uproject: string | null) => void;
    onForget: (uproject: string) => void;
    onDownload: () => void;
}

export default function Details(props: Props) {
    const { asset, recent, busy, overwrite, onOverwrite } = props;
    const { onAdd, onForget, onDownload } = props;

    if (!asset)
        return (
            <aside className="details empty">
                Choisis un asset pour voir ses versions d'Unreal et l'ajouter à un
                projet.
            </aside>
        );

    return (
        <aside className="details">
            <Thumb url={asset.thumbnail} large />
            <h2>{asset.title}</h2>
            <p className="kind">
                <KindPin kind={asset.kind} />
                {KIND_LABEL[asset.kind]}
                {asset.category && <span className="muted">{asset.category}</span>}
            </p>

            <h3>Versions d'Unreal</h3>
            <ul className="versions">
                {asset.engines.map((v) => (
                    <li key={v}>{v}</li>
                ))}
            </ul>

            {asset.kind === "project" ? (
                <p className="muted actions">
                    C'est un projet complet : télécharge-le dans un dossier, puis ouvre-le
                    dans Unreal.
                </p>
            ) : (
                <section className="actions">
                    <h3>Ajouter à un projet</h3>
                    {recent.length > 0 && (
                        <ul className="recent">
                            {recent.map((p) => (
                                <li key={p.path}>
                                    <button
                                        className="project"
                                        title={p.path}
                                        disabled={busy}
                                        onClick={() => onAdd(p.path)}
                                    >
                                        <span className="exec" aria-hidden="true" />
                                        <span className="project-name">{p.name}</span>
                                        {p.engine && <span className="muted">UE {p.engine}</span>}
                                    </button>
                                    <button
                                        className="quiet forget"
                                        title="Retirer de la liste"
                                        aria-label={`Retirer ${p.name} de la liste`}
                                        onClick={() => onForget(p.path)}
                                    >
                                        ×
                                    </button>
                                </li>
                            ))}
                        </ul>
                    )}
                    <button
                        className={recent.length > 0 ? "" : "primary"}
                        disabled={busy}
                        onClick={() => onAdd(null)}
                    >
                        {recent.length > 0
                            ? "Choisir un autre projet…"
                            : "Choisir un projet…"}
                    </button>
                    <label className="check">
                        <input
                            type="checkbox"
                            checked={overwrite}
                            onChange={(e) => onOverwrite(e.target.checked)}
                        />
                        Écraser les fichiers déjà présents
                    </label>
                </section>
            )}

            <button disabled={busy} onClick={onDownload}>
                Télécharger dans un dossier…
            </button>
        </aside>
    );
}