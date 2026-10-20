// L'inspecteur de l'écran Moteurs : la version choisie dans la grille. Installée, on
// la lance, et on voit si une plus récente de sa branche est sortie ; dans la file, on
// suit son avancement et sa vitesse ; sinon, on voit la place qu'elle prendra avant de
// l'installer.
import {
    branchOf,
    formatBytes,
    formatDay,
    formatDuration,
    formatRate,
    progressRatio,
    progressText,
    tilde,
    type AvailableEngine,
    type LocalEngine,
    type QueueJob,
} from "../api";
import type { Rate } from "../rates";
import { ProgressBar, SpeedGraph } from "./Common";

interface Props {
    engine: AvailableEngine | undefined;
    installed: LocalEngine | undefined;
    // La version plus récente de la même branche, si celle-ci est installée et en a une
    newer: AvailableEngine | undefined;
    // Son installation dans la file, s'il y en a une
    newerJob: QueueJob | undefined;
    job: QueueJob | undefined;
    rate: Rate | undefined;
    folder: string;
    // Place libre dans le dossier des moteurs, `null` tant qu'on ne la connaît pas
    free: number | null;
    onInstall: (version: string) => void;
    // Choisir une autre version dans la grille
    onSelect: (version: string) => void;
    onLaunch: (engine: LocalEngine) => void;
    onPause: (id: number) => void;
    onResume: (id: number) => void;
    onRemove: (id: number) => void;
}

export default function EngineInspector(props: Props) {
    const { engine, installed, job } = props;
    if (!engine)
        return (
            <aside className="inspector empty">
                Choisis une version dans la grille pour l'installer, ou pour suivre son
                installation.
            </aside>
        );

    return (
        <aside className="inspector" aria-label="Version choisie">
            <div>
                <h2>Unreal Engine {engine.version}</h2>
                {engine.date && (
                    <p className="muted">Publiée le {formatDay(engine.date)}</p>
                )}
            </div>
            {installed ? (
                <>
                    <section className="block">
                        <h3>Sur ce disque</h3>
                        <p className="path" title={installed.dir}>
                            {tilde(installed.dir)}
                        </p>
                        <div className="buttons">
                            <button
                                className={installed.update ? undefined : "primary"}
                                onClick={() => props.onLaunch(installed)}
                            >
                                Lancer
                            </button>
                        </div>
                        {installed.replacedBy && (
                            <p className="muted">
                                La {installed.replacedBy} est installée : uehub ouvre maintenant
                                tes projets {branchOf(installed.replacedBy)} avec elle.
                                {installed.byUehub &&
                                    " Tu peux désinstaller celle-ci depuis la liste, pour libérer sa place."}
                            </p>
                        )}
                    </section>
                    {installed.update && (
                        <UpdateBlock
                            {...props}
                            installed={installed}
                            version={installed.update}
                        />
                    )}
                </>
            ) : job ? (
                <JobBlock {...props} job={job} />
            ) : (
                <InstallBlock {...props} engine={engine} />
            )}
        </aside>
    );
}

// Epic a publié une version plus récente de la même branche : elle s'installe à côté.
function UpdateBlock(
    props: Props & { installed: LocalEngine; version: string },
) {
    const { installed, version, newer, newerJob, onInstall, onSelect } = props;
    return (
        <section className="block">
            <h3>Mise à jour</h3>
            <p>
                La {version} est sortie
                {newer?.date && ` le ${formatDay(newer.date)}`}. Elle s'installe à côté
                de la {installed.version}, et uehub ouvrira ensuite tes projets{" "}
                {branchOf(version)} avec elle.
            </p>
            <div className="buttons">
                {newerJob ? (
                    <button onClick={() => onSelect(version)}>
                        Suivre l'installation
                    </button>
                ) : (
                    <>
                        <button className="primary" onClick={() => onInstall(version)}>
                            Installer {version}
                        </button>
                        <button className="quiet" onClick={() => onSelect(version)}>
                            Voir la {version}
                        </button>
                    </>
                )}
            </div>
        </section>
    );
}

// Une installation dans la file : en cours, en attente, en pause ou en échec.
function JobBlock(props: Props & { job: QueueJob }) {
    const { job, rate, onPause, onResume, onRemove } = props;
    const remove = (
        <button className="quiet" onClick={() => onRemove(job.id)}>
            Retirer de la file
        </button>
    );

    if (job.status.state === "running") {
        const extracting = job.progress?.phase === "files";
        return (
            <section className="block">
                <div className="split">
                    <strong>{extracting ? "Extraction" : "Téléchargement"}</strong>
                    <span className="num">
            {Math.floor(progressRatio(job.progress) * 100)} %
          </span>
                </div>
                <ProgressBar progress={job.progress} />
                <div className="split muted">
                    <span>{progressText(job.progress)}</span>
                    {rate?.remaining != null && (
                        <span>encore {formatDuration(rate.remaining)}</span>
                    )}
                </div>
                {rate?.speed != null && <Speed rate={rate} speed={rate.speed} />}
                <div className="buttons">
                    <button onClick={() => onPause(job.id)}>Pause</button>
                    {remove}
                </div>
            </section>
        );
    }

    const failed = job.status.state === "failed" ? job.status.error : null;
    return (
        <section className="block">
            {failed ? (
                <p className="error">{failed}</p>
            ) : (
                <p>
                    {job.status.state === "paused"
                        ? "En pause : le téléchargement reprendra là où il s'est arrêté."
                        : "En attente : l'installation commencera à son tour dans la file."}
                </p>
            )}
            <div className="buttons">
                {job.status.state === "waiting" ? (
                    <button onClick={() => onPause(job.id)}>Pause</button>
                ) : (
                    <button onClick={() => onResume(job.id)}>
                        {failed ? "Réessayer" : "Reprendre"}
                    </button>
                )}
                {remove}
            </div>
        </section>
    );
}

// La vitesse maintenant, et celle de la dernière minute en barres.
function Speed({ rate, speed }: { rate: Rate; speed: number }) {
    const average =
        rate.history.length > 0
            ? rate.history.reduce((a, b) => a + b, 0) / rate.history.length
            : null;
    return (
        <div className="speed">
            <div className="split">
                <h3>Vitesse</h3>
                <span className="speed-now">{formatRate(speed)}</span>
            </div>
            <SpeedGraph history={rate.history} />
            <div className="split muted small">
                <span>Dernière minute</span>
                {average !== null && <span>moyenne {formatRate(average)}</span>}
            </div>
        </div>
    );
}

// Une version à installer : la place qu'elle prendra, puis où elle ira.
function InstallBlock(props: Props & { engine: AvailableEngine }) {
    const { engine, folder, free, onInstall } = props;
    const needed = engine.size + engine.extracted;
    // Les deux parts de la barre, en pourcentage de la place libre.
    const zip = free ? Math.min(engine.size / free, 1) : 0;
    const extracted = free ? Math.min(engine.extracted / free, 1 - zip) : 0;

    return (
        <>
            <section className="block">
                <h3>Place sur le disque</h3>
                {free !== null && (
                    <div
                        className="disk"
                        role="img"
                        aria-label={`Pendant l'installation : ${formatBytes(needed)} sur ${formatBytes(free)} libres`}
                    >
                        <span className="disk-zip" style={{ width: `${zip * 100}%` }} />
                        <span
                            className="disk-engine"
                            style={{ width: `${extracted * 100}%` }}
                        />
                    </div>
                )}
                <dl className="disk-legend">
                    <div>
                        <dt>
                            <span className="swatch disk-zip" />
                            Le zip
                        </dt>
                        <dd>{formatBytes(engine.size)}</dd>
                    </div>
                    <div>
                        <dt>
                            <span className="swatch disk-engine" />
                            Le moteur extrait
                        </dt>
                        <dd>environ {formatBytes(engine.extracted)}</dd>
                    </div>
                    {free !== null && (
                        <div className="muted">
                            <dt>
                                <span className="swatch disk-free" />
                                Libre ensuite
                            </dt>
                            <dd>{formatBytes(Math.max(free - engine.extracted, 0))}</dd>
                        </div>
                    )}
                </dl>
                <p className="muted small">
                    Le zip est supprimé une fois le moteur extrait.
                </p>
                {free !== null && free < needed && (
                    <p className="error">
                        Pas assez de place : il faut environ {formatBytes(needed)} libres,
                        il en reste {formatBytes(free)}.
                    </p>
                )}
            </section>
            <section className="block">
                <h3>Destination</h3>
                <p className="path">
                    {tilde(`${folder.replace(/\/$/, "")}/${engine.version}`)}
                </p>
                <div className="buttons">
                    <button className="primary" onClick={() => onInstall(engine.version)}>
                        Installer {engine.version}
                    </button>
                </div>
            </section>
        </>
    );
}