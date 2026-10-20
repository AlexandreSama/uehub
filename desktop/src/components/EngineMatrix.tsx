// La grille des versions d'Epic : une ligne par branche (5.8, 5.7…), une case par
// patch. On voit d'un coup d'œil ce qui est sur le disque, dans la file ou à installer.
import {
  branchOf,
  formatBytes,
  formatDay,
  jobFor,
  type AvailableEngine,
  type LocalEngine,
  type QueueJob,
} from "../api";
import { t, type Key } from "../i18n";

type CellState = "installed" | "queued" | "available";

// La légende, que le lecteur d'écran annonce aussi pour chaque case
const LEGEND: Record<CellState, Key> = {
  installed: "matrix.installed",
  queued: "matrix.queued",
  available: "matrix.available",
};

interface Branch {
  name: string;
  // Le numéro de patch → la version
  patches: Map<number, AvailableEngine>;
  // La plus récente de la branche : sa taille et sa date dans les deux dernières colonnes
  latest: AvailableEngine;
}

// La liste d'Epic arrive triée de la plus récente à la plus ancienne : les branches aussi.
function branches(engines: AvailableEngine[]): Branch[] {
  const byName = new Map<string, Branch>();
  for (const engine of engines) {
    const name = branchOf(engine.version);
    const patch = Number(engine.version.split(".")[2] ?? 0);
    const branch = byName.get(name);
    if (branch) branch.patches.set(patch, engine);
    else
      byName.set(name, {
        name,
        patches: new Map([[patch, engine]]),
        latest: engine,
      });
  }
  return [...byName.values()];
}

interface Props {
  engines: AvailableEngine[];
  local: LocalEngine[];
  jobs: QueueJob[];
  selected: string | null;
  onSelect: (version: string) => void;
}

export default function EngineMatrix(props: Props) {
  const { engines, local, jobs, selected, onSelect } = props;
  const rows = branches(engines);
  const columns = Math.max(
    1,
    ...rows.flatMap((b) => [...b.patches.keys()].map((p) => p + 1)),
  );

  function stateOf(version: string): CellState {
    if (local.some((l) => l.version === version)) return "installed";
    return jobFor(jobs, version) ? "queued" : "available";
  }

  return (
    <div className="matrix-box">
      <table className="matrix">
        <thead>
          <tr>
            <th scope="col">{t("matrix.branch")}</th>
            {Array.from({ length: columns }, (_, p) => (
              <th key={p} scope="col">
                .{p}
              </th>
            ))}
            <th scope="col" className="num">
              Zip
            </th>
            <th scope="col" className="num">
              {t("matrix.latest")}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((branch) => (
            <tr key={branch.name}>
              <th scope="row">{branch.name}</th>
              {Array.from({ length: columns }, (_, p) => {
                const engine = branch.patches.get(p);
                if (!engine) return <td key={p} />;
                const state = stateOf(engine.version);
                return (
                  <td key={p}>
                    <button
                      className={`cell ${state}`}
                      aria-pressed={engine.version === selected}
                      aria-label={`UE ${engine.version}, ${t(LEGEND[state])}`}
                      onClick={() => onSelect(engine.version)}
                    >
                      .{p}
                    </button>
                  </td>
                );
              })}
              <td className="num">{formatBytes(branch.latest.size)}</td>
              <td className="num muted">{formatDay(branch.latest.date)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// La légende des couleurs, à côté du titre de la grille.
export function MatrixLegend() {
  return (
    <span className="legend" aria-hidden="true">
      {(Object.keys(LEGEND) as CellState[]).map((state) => (
        <span key={state}>
          <span className={`swatch ${state}`} />
          {t(LEGEND[state])}
        </span>
      ))}
    </span>
  );
}
