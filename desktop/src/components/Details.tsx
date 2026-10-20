// L'inspecteur de l'asset choisi : ses versions d'Unreal, et où l'ajouter.
import { useEffect, useRef, useState } from "react";
import {
  compareVersions,
  engineRange,
  formatSeconds,
  kindLabel,
  updateText,
  type AddedAsset,
  type Asset,
  type RecentProject,
} from "../api";
import { t } from "../i18n";
import { KindPin, Thumb } from "./Common";
import type { Notice } from "./Library";

interface Props {
  asset: Asset | null;
  // Les versions de toute la bibliothèque, pour la règle
  versions: string[];
  recent: RecentProject[];
  // Les projets où uehub a déjà ajouté cet asset
  added: AddedAsset[];
  // Un refus de la file, ou le bilan d'un retrait
  notice: Notice | null;
  overwrite: boolean;
  onOverwrite: (value: boolean) => void;
  // `null` : ouvrir la boîte de dialogue pour choisir le projet
  onAdd: (uproject: string | null) => void;
  onUpdate: (uproject: string) => void;
  // Efface les fichiers de l'asset dans ce projet, puis sa fiche
  onRemove: (uproject: string) => void;
  onOpenFab: () => void;
  onForget: (uproject: string) => void;
  onDownload: () => void;
}

// Ce que donnerait l'ajout à un projet, avec la même règle que plan_add côté Rust :
// la version exacte, sinon une plus ancienne qu'Unreal convertira (pas pour un plugin).
type Fit =
  | { kind: "exact" }
  | { kind: "older"; version: string }
  | { kind: "none"; reason: string };

function fitFor(asset: Asset, engine: string | null): Fit {
  if (!engine) return { kind: "none", reason: t("details.no_project_version") };
  if (asset.engines.includes(engine)) return { kind: "exact" };
  const older = asset.engines.filter((v) => compareVersions(v, engine) < 0);
  if (older.length === 0)
    return { kind: "none", reason: t("details.nothing_for", { engine }) };
  if (asset.kind === "plugin")
    return { kind: "none", reason: t("errors.plugin_not_for", { engine }) };
  return { kind: "older", version: older[older.length - 1] };
}

function fitNote(fit: Fit, engine: string | null): string | null {
  if (fit.kind === "older")
    return t("details.older", { engine: engine ?? "?", version: fit.version });
  return fit.kind === "none" ? fit.reason : null;
}

export default function Details(props: Props) {
  const { asset, versions, recent, added, notice, overwrite, onOverwrite } =
    props;
  const { onAdd, onUpdate, onRemove, onOpenFab, onForget, onDownload } = props;
  const noticeRef = useRef<HTMLParagraphElement>(null);
  // Le projet dont on demande confirmation avant d'y effacer l'asset
  const [confirming, setConfirming] = useState<string | null>(null);

  // Un autre asset choisi : la confirmation en cours ne le concerne pas.
  useEffect(() => {
    setConfirming(null);
  }, [asset?.id]);

  // Le message s'affiche sous les boutons : on le fait défiler à l'écran s'il le faut.
  useEffect(() => {
    noticeRef.current?.scrollIntoView({ block: "nearest" });
  }, [notice]);

  if (!asset)
    return <aside className="details empty">{t("details.empty")}</aside>;

  // La fiche de l'asset dans un projet, s'il y est déjà.
  const addedIn = (p: RecentProject) => added.find((a) => a.project === p.path);
  // Le bouton mis en avant : une mise à jour s'il y en a une, sinon le premier projet
  // récent où l'asset va tel quel.
  const anyUpdate = added.some((a) => a.update);
  const best = anyUpdate
    ? undefined
    : recent.find(
        (p) => !addedIn(p) && fitFor(asset, p.engine).kind === "exact",
      );

  return (
    <aside className="details" aria-label={t("details.title")}>
      <Thumb url={asset.thumbnail} large />
      <div>
        <h2>{asset.title}</h2>
        <p className="kind">
          <KindPin kind={asset.kind} />
          {kindLabel(asset.kind)}
          {asset.category && <span className="muted">{asset.category}</span>}
          <button className="quiet fab-link" onClick={onOpenFab}>
            {t("details.view_on_fab")}
          </button>
        </p>
      </div>

      <h3>{t("details.versions")}</h3>
      <Ruler asset={asset} versions={versions} />

      {asset.kind === "project" ? (
        <p className="muted">{t("details.complete_project")}</p>
      ) : (
        <section className="add">
          <h3>{t("details.add")}</h3>
          {recent.length > 0 && (
            <ul className="recent">
              {recent.map((p) => {
                const here = addedIn(p);
                const fit = fitFor(asset, p.engine);
                const note = here
                  ? here.update
                    ? updateText(here.update, here.engine)
                    : t("details.added_on", {
                        date: formatSeconds(here.addedAt),
                        engine: here.engine,
                      })
                  : fitNote(fit, p.engine);
                return (
                  <li key={p.path}>
                    {here?.update ? (
                      <button
                        className="project primary"
                        title={p.path}
                        onClick={() => onUpdate(p.path)}
                      >
                        <span className="project-name">{p.name}</span>
                        <span>{t("details.update")}</span>
                      </button>
                    ) : (
                      <button
                        className={p === best ? "project primary" : "project"}
                        title={p.path}
                        disabled={!here && fit.kind === "none"}
                        onClick={() => onAdd(p.path)}
                      >
                        <span className="project-name">{p.name}</span>
                        <span>
                          {here
                            ? t("details.up_to_date")
                            : p.engine && `UE ${p.engine}`}
                        </span>
                      </button>
                    )}
                    <button
                      className="quiet forget"
                      title={t("details.forget")}
                      aria-label={t("details.forget_project", { name: p.name })}
                      onClick={() => onForget(p.path)}
                    >
                      ×
                    </button>
                    {here && confirming === p.path ? (
                      <div className="confirm">
                        <span>
                          {t("details.confirm_remove", {
                            n: here.files,
                            project: p.name,
                          })}
                        </span>
                        <button
                          className="danger"
                          onClick={() => {
                            setConfirming(null);
                            onRemove(p.path);
                          }}
                        >
                          {t("common.delete")}
                        </button>
                        <button
                          className="quiet"
                          onClick={() => setConfirming(null)}
                        >
                          {t("common.cancel")}
                        </button>
                      </div>
                    ) : (
                      note && (
                        <p className="fit">
                          {note}
                          {here && (
                            <button
                              className="quiet inline-action"
                              onClick={() => setConfirming(p.path)}
                            >
                              {t("details.remove")}
                            </button>
                          )}
                        </p>
                      )
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          <button onClick={() => onAdd(null)}>
            {recent.length > 0
              ? t("details.other_project")
              : t("details.choose_project")}
          </button>
          <label className="check">
            <input
              type="checkbox"
              checked={overwrite}
              onChange={(e) => onOverwrite(e.target.checked)}
            />
            {t("details.overwrite")}
          </label>
        </section>
      )}

      <button onClick={onDownload}>{t("details.download")}</button>
      {notice && (
        <p
          ref={noticeRef}
          className={notice.error ? "error" : "muted"}
          role={notice.error ? "alert" : "status"}
        >
          {notice.text}
        </p>
      )}
    </aside>
  );
}

// Au plus 14 cases : au-delà, elles deviennent trop étroites pour leur numéro.
const RULER = 14;

// La règle des versions : celles de la bibliothèque à partir de la première de
// l'asset. Les cases pleines sont celles que l'asset couvre.
function Ruler({ asset, versions }: { asset: Asset; versions: string[] }) {
  const first = versions.indexOf(asset.engines[0]);
  const cells = versions.slice(Math.max(first, versions.length - RULER, 0));
  return (
    <div
      className="ruler"
      role="img"
      aria-label={t("details.ruler", { range: engineRange(asset.engines) })}
      style={{ gridTemplateColumns: `repeat(${cells.length}, minmax(0, 1fr))` }}
    >
      {cells.map((v) => (
        <span key={v} className={asset.engines.includes(v) ? "on" : undefined}>
          {v}
        </span>
      ))}
    </div>
  );
}
