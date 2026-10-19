// Petits éléments réutilisés : la vignette (ou la marque d'un moteur), la pastille
// du type d'asset, la barre de progression, le graphique de vitesse et les icônes.
import { useState, type ReactNode } from "react";
import {
    progressRatio,
    thumbnailSrc,
    type AssetKind,
    type Progress,
} from "../api";
import { HISTORY } from "../rates";

export function Thumb({
                          url,
                          large = false,
                      }: {
    url: string | null;
    large?: boolean;
}) {
    const [failed, setFailed] = useState<string | null>(null);
    const className = large ? "thumb thumb-large" : "thumb";
    // Pas d'image, ou image introuvable (hors ligne et pas encore en cache) : un simple fond.
    if (!url || failed === url) return <div className={className} />;
    return (
        <img
            className={className}
            src={thumbnailSrc(url)}
            alt=""
            loading="lazy"
            onError={() => setFailed(url)}
        />
    );
}

// Un moteur n'a pas de vignette : sa version, à la place.
export function EngineMark({ version }: { version: string }) {
    return (
        <div className="thumb engine-mark" aria-hidden="true">
            UE {version}
        </div>
    );
}

// Une pastille de couleur par type : ocre pour les packs, vert pour les plugins,
// violet pour les projets complets.
export function KindPin({ kind }: { kind: AssetKind }) {
    return <span className={`pin pin-${kind}`} aria-hidden="true" />;
}

export function ProgressBar({ progress }: { progress: Progress | null }) {
    return (
        <div
            className="bar"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={progress?.total ?? 0}
            aria-valuenow={progress?.done ?? 0}
        >
            <div style={{ width: `${progressRatio(progress) * 100}%` }} />
        </div>
    );
}

// La vitesse de la dernière minute en barres, la plus récente à droite.
export function SpeedGraph({ history }: { history: number[] }) {
    const max = Math.max(...history, 1);
    const empty = HISTORY - history.length;
    return (
        <div className="speed-graph" aria-hidden="true">
            {Array.from({ length: HISTORY }, (_, i) => (
                <span
                    key={i}
                    style={{
                        height: i < empty ? 0 : `${(history[i - empty] / max) * 100}%`,
                    }}
                />
            ))}
        </div>
    );
}

// Les icônes : des traits, qui prennent la couleur du texte.
function Icon({ children }: { children: ReactNode }) {
    return (
        <svg
            width="20"
            height="20"
            viewBox="0 0 20 20"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            aria-hidden="true"
        >
            {children}
        </svg>
    );
}

export function LibraryIcon() {
    return (
        <Icon>
            <rect x="2.5" y="2.5" width="6" height="6" rx="1" />
            <rect x="11.5" y="2.5" width="6" height="6" rx="1" />
            <rect x="2.5" y="11.5" width="6" height="6" rx="1" />
            <rect x="11.5" y="11.5" width="6" height="6" rx="1" />
        </Icon>
    );
}

export function EngineIcon() {
    return (
        <Icon>
            <path d="M10 2.5 L16.5 6.25 V13.75 L10 17.5 L3.5 13.75 V6.25 Z" />
            <path d="M10 10 L16.5 6.25 M10 10 V17.5 M10 10 L3.5 6.25" />
        </Icon>
    );
}

// Une flèche qui monte au-dessus d'un socle : une nouvelle version à installer.
export function UpdateIcon() {
    return (
        <Icon>
            <path d="M10 13.5 V3 M5.5 7.5 L10 3 L14.5 7.5" />
            <path d="M3.5 17 H16.5" />
        </Icon>
    );
}

export function SearchIcon() {
    return (
        <Icon>
            <circle cx="8.5" cy="8.5" r="5" />
            <path d="M12.5 12.5 L17 17" />
        </Icon>
    );
}