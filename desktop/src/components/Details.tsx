// L'inspecteur de l'asset choisi : ses versions d'Unreal, et où l'ajouter.
import { useEffect, useRef } from "react";
import {
    compareVersions,
    engineRange,
    KIND_LABEL,
    type Asset,
    type RecentProject,
} from "../api";
import { KindPin, Thumb } from "./Common";

interface Props {
    asset: Asset | null;
    // Les versions de toute la bibliothèque, pour la règle
    versions: string[];
    recent: RecentProject[];
    // Pourquoi la file a refusé la dernière demande, s'il y a lieu
    notice: string | null;
    overwrite: boolean;
    onOverwrite: (value: boolean) => void;
    // `null` : ouvrir la boîte de dialogue pour choisir le projet
    onAdd: (uproject: string | null) => void;
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
    if (!engine) return { kind: "none", reason: "Version du projet inconnue." };
    if (asset.engines.includes(engine)) return { kind: "exact" };
    const older = asset.engines.filter((v) => compareVersions(v, engine) < 0);
    if (older.length === 0)
        return { kind: "none", reason: `Rien pour UE ${engine} ni avant.` };
    if (asset.kind === "plugin")
        return {
            kind: "none",
            reason: `Ce plugin n'existe pas pour UE ${engine}.`,
        };
    return { kind: "older", version: older[older.length - 1] };
}

function fitNote(fit: Fit, engine: string | null): string | null {
    if (fit.kind === "older")
        return `Pas de version ${engine} : celle pour ${fit.version} sera installée, Unreal la convertira.`;
    return fit.kind === "none" ? fit.reason : null;
}

export default function Details(props: Props) {
    const { asset, versions, recent, notice, overwrite, onOverwrite } = props;
    const { onAdd, onForget, onDownload } = props;
    const noticeRef = useRef<HTMLParagraphElement>(null);

    // Le message s'affiche sous les boutons : on le fait défiler à l'écran s'il le faut.
    useEffect(() => {
        noticeRef.current?.scrollIntoView({ block: "nearest" });
    }, [notice]);

    if (!asset)
        return (
            <aside className="details empty">
                Choisis un asset pour voir ses versions d'Unreal et l'ajouter à un
                projet.
            </aside>
        );

    // Le premier projet récent où l'asset va tel quel : c'est le bouton mis en avant.
    const best = recent.find((p) => fitFor(asset, p.engine).kind === "exact");

    return (
        <aside className="details" aria-label="Détail de l'asset">
            <Thumb url={asset.thumbnail} large />
            <div>
                <h2>{asset.title}</h2>
                <p className="kind">
                    <KindPin kind={asset.kind} />
                    {KIND_LABEL[asset.kind]}
                    {asset.category && <span className="muted">{asset.category}</span>}
                </p>
            </div>

            <h3>Versions d'Unreal</h3>
            <Ruler asset={asset} versions={versions} />

            {asset.kind === "project" ? (
                <p className="muted">
                    C'est un projet complet : télécharge-le dans un dossier, puis ouvre-le
                    dans Unreal.
                </p>
            ) : (
                <section className="add">
                    <h3>Ajouter à un projet</h3>
                    {recent.length > 0 && (
                        <ul className="recent">
                            {recent.map((p) => {
                                const fit = fitFor(asset, p.engine);
                                const note = fitNote(fit, p.engine);
                                return (
                                    <li key={p.path}>
                                        <button
                                            className={p === best ? "project primary" : "project"}
                                            title={p.path}
                                            disabled={fit.kind === "none"}
                                            onClick={() => onAdd(p.path)}
                                        >
                                            <span className="project-name">{p.name}</span>
                                            {p.engine && <span>UE {p.engine}</span>}
                                        </button>
                                        <button
                                            className="quiet forget"
                                            title="Retirer de la liste"
                                            aria-label={`Retirer ${p.name} de la liste`}
                                            onClick={() => onForget(p.path)}
                                        >
                                            ×
                                        </button>
                                        {note && <p className="fit">{note}</p>}
                                    </li>
                                );
                            })}
                        </ul>
                    )}
                    <button onClick={() => onAdd(null)}>
                        {recent.length > 0 ? "Autre projet…" : "Choisir un projet…"}
                    </button>
                    <label className="check">
                        <input
                            type="checkbox"
                            checked={overwrite}
                            onChange={(e) => onOverwrite(e.target.checked)}
                        />
                        Écraser les fichiers déjà présents
                    </label>
                </section>
            )}

            <button onClick={onDownload}>Télécharger dans un dossier…</button>
            {notice && (
                <p ref={noticeRef} className="error" role="alert">
                    {notice}
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
            aria-label={`Compatible avec Unreal ${engineRange(asset.engines)}`}
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