// La colonne des filtres : le type d'asset, la version d'Unreal, et les projets
// récents, qui choisissent d'un clic la version de leur moteur.
import { useState } from "react";
import type { Asset, AssetKind, RecentProject } from "../api";
import { KindPin } from "./Common";

export type KindFilter = "all" | AssetKind;

const KINDS: { id: KindFilter; label: string }[] = [
    { id: "all", label: "Tout" },
    { id: "pack", label: "Packs d'assets" },
    { id: "plugin", label: "Plugins" },
    { id: "project", label: "Projets complets" },
];

// Les versions affichées d'emblée : les plus récentes.
const FIRST_VERSIONS = 8;

interface Props {
    assets: Asset[];
    // De la plus ancienne à la plus récente
    versions: string[];
    recent: RecentProject[];
    kind: KindFilter;
    onKind: (kind: KindFilter) => void;
    version: string | null;
    onVersion: (version: string | null) => void;
}

export default function Filters(props: Props) {
    const { assets, versions, recent, kind, onKind, version, onVersion } = props;
    const [allVersions, setAllVersions] = useState(false);
    const newestFirst = [...versions].reverse();
    const shown = allVersions
        ? newestFirst
        : newestFirst.slice(0, FIRST_VERSIONS);
    const projects = recent.filter((p) => p.engine !== null);

    const count = (k: KindFilter) =>
        k === "all" ? assets.length : assets.filter((a) => a.kind === k).length;
    // Un second clic sur la version choisie la retire.
    const toggle = (v: string) => onVersion(version === v ? null : v);

    return (
        <aside className="filters" aria-label="Filtres">
            <section>
                <h2>Type</h2>
                {KINDS.map((k) => (
                    <button
                        key={k.id}
                        className="filter"
                        aria-pressed={kind === k.id}
                        onClick={() => onKind(k.id)}
                    >
                        {k.id !== "all" && <KindPin kind={k.id} />}
                        <span className="filter-label">{k.label}</span>
                        <span className="muted">{count(k.id)}</span>
                    </button>
                ))}
            </section>

            {versions.length > 0 && (
                <section>
                    <h2>Compatible avec</h2>
                    <div className="chips">
                        {shown.map((v) => (
                            <button
                                key={v}
                                className="chip"
                                aria-pressed={version === v}
                                onClick={() => toggle(v)}
                            >
                                {v}
                            </button>
                        ))}
                    </div>
                    {versions.length > FIRST_VERSIONS && (
                        <button
                            className="quiet more"
                            onClick={() => setAllVersions(!allVersions)}
                        >
                            {allVersions
                                ? "Seulement les récentes"
                                : `Toutes les versions (${versions.length})`}
                        </button>
                    )}
                </section>
            )}

            {projects.length > 0 && (
                <section>
                    <h2>Mes projets</h2>
                    {projects.map((p) => (
                        <button
                            key={p.path}
                            className="filter"
                            title={p.path}
                            aria-pressed={version === p.engine}
                            onClick={() => toggle(p.engine!)}
                        >
                            <span className="filter-label">{p.name}</span>
                            <span className="muted">{p.engine}</span>
                        </button>
                    ))}
                </section>
            )}
        </aside>
    );
}