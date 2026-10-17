// Petits éléments réutilisés : la vignette et le « pin » coloré qui code le type d'asset.
import { useEffect, useRef, useState } from "react";
import { thumbnailSrc, type AssetKind } from "../api";

export function Thumb({
                          url,
                          large = false,
                      }: {
    url: string | null;
    large?: boolean;
}) {
    const ref = useRef<HTMLDivElement>(null);
    const [visible, setVisible] = useState(false);
    const src = useThumbnail(visible ? url : null);
    const className = large ? "thumb thumb-large" : "thumb";

    // Chargement paresseux : la vignette n'est demandée qu'en approchant de l'écran.
    useEffect(() => {
        const el = ref.current;
        if (!el || visible) return;
        const observer = new IntersectionObserver(
            (entries) => {
                if (entries.some((e) => e.isIntersecting)) setVisible(true);
            },
            { rootMargin: "200px" },
        );
        observer.observe(el);
        return () => observer.disconnect();
    }, [visible]);

    // Pas d'image, pas encore chargée, ou introuvable (hors ligne et pas en cache) : un simple fond.
    if (!src) return <div ref={ref} className={className} />;
    return <img className={className} src={src} alt="" />;
}

// WebKitGTK refuse d'afficher dans un <img> une adresse thumb:// depuis la page
// (« Domains, protocols and ports must match »), mais fetch y a droit.
// On récupère donc les octets et on les affiche via une URL blob:.
function useThumbnail(url: string | null): string | null {
    const [loaded, setLoaded] = useState<{ url: string; src: string } | null>(null);

    useEffect(() => {
        if (!url) return;
        let objectUrl: string | null = null;
        let cancelled = false;
        fetch(thumbnailSrc(url))
            .then((r) => (r.ok ? r.blob() : Promise.reject(r.status)))
            .then((b) => {
                if (cancelled) return;
                objectUrl = URL.createObjectURL(b);
                setLoaded({ url, src: objectUrl });
            })
            .catch(() => {});
        return () => {
            cancelled = true;
            if (objectUrl) URL.revokeObjectURL(objectUrl);
        };
    }, [url]);

    return loaded && loaded.url === url ? loaded.src : null;
}

// Les couleurs reprennent celles des pins de Blueprint d'Unreal.
export function KindPin({ kind }: { kind: AssetKind }) {
    return <span className={`pin pin-${kind}`} aria-hidden="true" />;
}
