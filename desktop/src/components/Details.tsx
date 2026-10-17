import { KIND_LABEL, type Asset } from "../api";
import { KindPin, Thumb } from "./Common";

interface Props {
    asset: Asset | null;
    busy: boolean;
    overwrite: boolean;
    onOverwrite: (value: boolean) => void;
    onAdd: () => void;
    onDownload: () => void;
}

export default function Details(props: Props) {
    const { asset, busy, overwrite, onOverwrite, onAdd, onDownload } = props;

    if (!asset)
        return (
            <aside className="details empty">
                Choisis un asset pour voir ses versions d'Unreal et l'ajouter à un
                projet.
            </aside>
        );

    const canAdd = asset.kind !== "project";

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

            <div className="actions">
                {canAdd ? (
                    <button className="primary" disabled={busy} onClick={onAdd}>
                        Ajouter à un projet…
                    </button>
                ) : (
                    <p className="muted">
                        C'est un projet complet : télécharge-le dans un dossier, puis
                        ouvre-le dans Unreal.
                    </p>
                )}
                <button disabled={busy} onClick={onDownload}>
                    Télécharger dans un dossier…
                </button>
                {canAdd && (
                    <label className="check">
                        <input
                            type="checkbox"
                            checked={overwrite}
                            onChange={(e) => onOverwrite(e.target.checked)}
                        />
                        Écraser les fichiers déjà présents
                    </label>
                )}
            </div>
        </aside>
    );
}