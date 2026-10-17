import { engineRange, KIND_LABEL, type Asset } from "../api";
import { KindPin, Thumb } from "./Common";

interface Props {
    loading: boolean;
    error: string | null;
    onRetry: () => void;
    assets: Asset[];
    selectedId: string | null;
    onSelect: (id: string) => void;
}

export default function AssetGrid(props: Props) {
    const { loading, error, onRetry, assets, selectedId, onSelect } = props;

    if (loading)
        return (
            <section className="assets empty">Chargement de ta bibliothèque…</section>
        );
    if (error)
        return (
            <section className="assets empty">
                <p>Impossible de charger ta bibliothèque : {error}</p>
                <button onClick={onRetry}>Réessayer</button>
            </section>
        );
    if (assets.length === 0)
        return (
            <section className="assets empty">
                Aucun asset ne correspond à ta recherche.
            </section>
        );

    return (
        <section className="assets">
            <ul className="grid">
                {assets.map((asset) => (
                    <li key={asset.id}>
                        <button
                            className="card"
                            aria-pressed={asset.id === selectedId}
                            onClick={() => onSelect(asset.id)}
                        >
                            <Thumb url={asset.thumbnail} />
                            <span className="card-title">{asset.title}</span>
                            <span className="card-meta">
                <KindPin kind={asset.kind} />
                <span>{KIND_LABEL[asset.kind]}</span>
                <span className="range">{engineRange(asset.engines)}</span>
              </span>
                        </button>
                    </li>
                ))}
            </ul>
        </section>
    );
}