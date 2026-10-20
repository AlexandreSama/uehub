// La colonne des filtres : le suivi de ce qui est déjà dans tes projets, le type
// d'asset, la version d'Unreal, et les projets récents, qui choisissent d'un clic la
// version de leur moteur.
import { useState } from "react";
import type { Asset, AssetKind, RecentProject } from "../api";
import { t, type Key } from "../i18n";
import { KindPin } from "./Common";

export type KindFilter = "all" | AssetKind;

// Tout, seulement les assets déjà ajoutés à un projet, ou ceux qui ont une mise à jour.
export type Tracked = "all" | "added" | "updates";

const KINDS: { id: KindFilter; label: Key }[] = [
  { id: "all", label: "filters.all" },
  { id: "pack", label: "filters.packs" },
  { id: "plugin", label: "filters.plugins" },
  { id: "project", label: "filters.projects" },
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
  tracked: Tracked;
  onTracked: (tracked: Tracked) => void;
  addedCount: number;
  updateCount: number;
}

export default function Filters(props: Props) {
  const { assets, versions, recent, kind, onKind, version, onVersion } = props;
  const { tracked, onTracked, addedCount, updateCount } = props;
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
  // Pareil pour le suivi.
  const track = (t: Tracked) => onTracked(tracked === t ? "all" : t);

  return (
    <aside className="filters" aria-label={t("filters.title")}>
      {addedCount > 0 && (
        <section>
          <h2>{t("filters.tracking")}</h2>
          <button
            className="filter"
            aria-pressed={tracked === "updates"}
            onClick={() => track("updates")}
          >
            <span className="filter-label">{t("filters.updates")}</span>
            <span className={updateCount > 0 ? "update-count" : "muted"}>
              {updateCount}
            </span>
          </button>
          <button
            className="filter"
            aria-pressed={tracked === "added"}
            onClick={() => track("added")}
          >
            <span className="filter-label">{t("filters.added")}</span>
            <span className="muted">{addedCount}</span>
          </button>
        </section>
      )}

      <section>
        <h2>{t("library.col_type")}</h2>
        {KINDS.map((k) => (
          <button
            key={k.id}
            className="filter"
            aria-pressed={kind === k.id}
            onClick={() => onKind(k.id)}
          >
            {k.id !== "all" && <KindPin kind={k.id} />}
            <span className="filter-label">{t(k.label)}</span>
            <span className="muted">{count(k.id)}</span>
          </button>
        ))}
      </section>

      {versions.length > 0 && (
        <section>
          <h2>{t("filters.compatible")}</h2>
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
                ? t("filters.recent_only")
                : t("filters.all_versions", { n: versions.length })}
            </button>
          )}
        </section>
      )}

      {projects.length > 0 && (
        <section>
          <h2>{t("filters.my_projects")}</h2>
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
