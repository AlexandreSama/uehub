// Les moteurs : ceux de ce disque, la liste d'Epic, et leurs mises à jour. L'état vit
// dans App : le rail signale une mise à jour même quand l'écran Moteurs est fermé.
import { useEffect, useRef, useState } from "react";
import {
  api,
  type EngineCatalog,
  type LocalEngine,
  type QueueJob,
} from "./api";
import { RECHECK_MS, useTick } from "./recheck";

export interface EngineState {
  local: LocalEngine[];
  catalog: EngineCatalog | null;
  // Pourquoi la dernière lecture de la liste d'Epic a échoué
  error: string | null;
  refreshing: boolean;
  // Les moteurs installés dont Epic propose une version plus récente
  updates: LocalEngine[];
  reload: () => Promise<void>;
  refresh: () => Promise<void>;
}

// `ready` : la session Epic est là et la bibliothèque chargée. La liste d'Epic ne se
// relit en arrière-plan qu'à partir de là, pour ne pas retarder le démarrage.
export function useEngines(ready: boolean, jobs: QueueJob[]): EngineState {
  const [local, setLocal] = useState<LocalEngine[]>([]);
  const [catalog, setCatalog] = useState<EngineCatalog | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  // Une seule lecture à la fois, même si l'écran et la minuterie la demandent ensemble
  const busy = useRef(false);
  // La dernière tentative, réussie ou non : hors ligne, on ne réessaie pas toutes les
  // dix minutes
  const tried = useRef(0);

  async function reload() {
    try {
      setLocal(await api.localEngines());
    } catch {
      setLocal([]);
    }
  }

  // Relire la liste d'Epic passe par une session unrealengine.com : quelques secondes.
  // La liste affichée reste en place pendant ce temps.
  async function refresh() {
    if (busy.current) return;
    busy.current = true;
    tried.current = Date.now();
    setRefreshing(true);
    setError(null);
    try {
      setCatalog(await api.availableEngines());
    } catch (e) {
      setError(String(e));
    }
    setRefreshing(false);
    busy.current = false;
    // Les mises à jour se lisent dans la liste qu'on vient de garder sur le disque.
    await reload();
  }

  // Au démarrage, sans réseau : la liste gardée sur le disque.
  useEffect(() => {
    api
      .cachedEngines()
      // Si Epic a déjà répondu, sa liste est plus fraîche : on la garde.
      .then((cached) => setCatalog((shown) => shown ?? cached))
      .catch(() => {});
  }, []);

  // Les moteurs installés, au démarrage puis à chaque installation terminée.
  const finished = jobs.filter(
    (j) => j.request.kind === "installEngine" && j.status.state === "done",
  ).length;
  useEffect(() => {
    void reload();
  }, [finished]);

  // Relit la liste d'Epic si elle a plus de quatre heures. Seulement avec un moteur
  // installé : sans moteur, il n'y a pas de mise à jour à attendre.
  async function recheck() {
    if (!ready || busy.current) return;
    const [cached, installed] = await Promise.all([
      api.cachedEngines(),
      api.localEngines(),
    ]);
    if (!installed.some((e) => e.version)) return;
    const last = Math.max((cached?.checkedAt ?? 0) * 1000, tried.current);
    if (Date.now() - last < RECHECK_MS) return;
    await refresh();
  }

  useEffect(() => {
    recheck().catch(() => {});
  }, [ready]);
  useTick(() => {
    recheck().catch(() => {});
  });

  const updates = local.filter((e) => e.update !== null);
  return { local, catalog, error, refreshing, updates, reload, refresh };
}
