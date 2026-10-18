// La liste des assets, une ligne par asset : vignette, titre, type et versions d'Unreal.
import { engineRange, KIND_LABEL, type Asset } from "../api";
import { KindPin, Thumb } from "./Common";

interface Props {
    loading: boolean;
    error: string | null;
    onRetry: () => void;
    assets: Asset[];
    selectedId: string | null;
    onSelect: (id: string) => void;
    // Les assets qui ont une mise à jour dans au moins un projet
    updates: Set<string>;
}

export default function AssetList(props: Props) {
    const { loading, error, onRetry, assets, updates, selectedId, onSelect } =
        props;

    let body;
    if (loading)
        body = <p className="list-message">Chargement de ta bibliothèque…</p>;
    else if (error)
        body = (
            <div className="list-message">
                <p>Impossible de charger ta bibliothèque : {error}</p>
                <button onClick={onRetry}>Réessayer</button>
            </div>
        );
    else if (assets.length === 0)
        body = <p className="list-message">Aucun asset ne correspond.</p>;
    else
        body = (
            <ul>
                {assets.map((asset) => (
                    <li key={asset.id}>
                        <button
                            className="asset-row"
                            aria-pressed={asset.id === selectedId}
                            onClick={() => onSelect(asset.id)}
                        >
                            <Thumb url={asset.thumbnail} />
                            <span className="asset-name">
                                <span className="asset-title">
                                      {updates.has(asset.id) && (
                                          <span className="badge">Mise à jour</span>
                                      )}
                                                        {asset.title}
                                </span>
                                {asset.category && (
                                    <span className="muted">{asset.category}</span>
                                )}
              </span>
                            <span className="asset-kind">
                <KindPin kind={asset.kind} />
                                {KIND_LABEL[asset.kind]}
              </span>
                            <span className="asset-range">{engineRange(asset.engines)}</span>
                        </button>
                    </li>
                ))}
            </ul>
        );

    return (
        <section className="assets" aria-label="Assets">
            <div className="list-head" aria-hidden="true">
                <span />
                <span>Asset</span>
                <span>Type</span>
                <span>Unreal</span>
            </div>
            {body}
        </section>
    );
}