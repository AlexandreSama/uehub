// Petits éléments réutilisés : la vignette et le « pin » coloré qui code le type d'asset.
import { thumbnailSrc, type AssetKind } from "../api";

export function Thumb({
                          url,
                          large = false,
                      }: {
    url: string | null;
    large?: boolean;
}) {
    const className = large ? "thumb thumb-large" : "thumb";
    if (!url) return <div className={className} />;
    return (
        <img className={className} src={thumbnailSrc(url)} alt="" loading="lazy" />
    );
}

// Les couleurs reprennent celles des pins de Blueprint d'Unreal.
export function KindPin({ kind }: { kind: AssetKind }) {
    return <span className={`pin pin-${kind}`} aria-hidden="true" />;
}