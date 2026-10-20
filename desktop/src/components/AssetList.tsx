// La liste des assets, une ligne par asset : vignette, titre, type et versions d'Unreal.
import { engineRange, kindLabel, type Asset } from "../api";
import { t } from "../i18n";
import { KindPin, Thumb } from "./Common";
import LogButton from "./LogButton";

interface Props {
  loading: boolean;
  error: string | null;
  onRetry: () => void;
  assets: Asset[];
  // Les assets qui ont une mise à jour dans au moins un projet
  updates: Set<string>;
  selectedId: string | null;
  onSelect: (id: string) => void;
}

export default function AssetList(props: Props) {
  const { loading, error, onRetry, assets, updates, selectedId, onSelect } =
    props;

  let body;
  if (loading) body = <p className="list-message">{t("library.loading")}</p>;
  else if (error)
    body = (
      <div className="list-message">
        <p>
          {t("library.load_failed")} {error}
        </p>
        <div className="buttons">
          <button onClick={onRetry}>{t("common.retry")}</button>
          <LogButton />
        </div>
      </div>
    );
  else if (assets.length === 0)
    body = <p className="list-message">{t("library.no_match")}</p>;
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
                    <span className="badge">{t("common.update")}</span>
                  )}
                  {asset.title}
                </span>
                {asset.category && (
                  <span className="muted">{asset.category}</span>
                )}
              </span>
              <span className="asset-kind">
                <KindPin kind={asset.kind} />
                {kindLabel(asset.kind)}
              </span>
              <span className="asset-range">{engineRange(asset.engines)}</span>
            </button>
          </li>
        ))}
      </ul>
    );

  return (
    <section className="assets" aria-label={t("library.assets")}>
      <div className="list-head" aria-hidden="true">
        <span />
        <span>{t("library.col_asset")}</span>
        <span>{t("library.col_type")}</span>
        <span>Unreal</span>
      </div>
      {body}
    </section>
  );
}
