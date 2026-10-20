// La vitesse des téléchargements. Rust envoie seulement les octets reçus : on les
// relève une fois par seconde et on en tire la vitesse, son historique et le temps restant.
import { useEffect, useRef, useState } from "react";
import type { QueueJob } from "./api";

export interface Rate {
  // Octets par seconde sur les 5 dernières secondes, `null` le temps de mesurer
  speed: number | null;
  // Une vitesse toutes les 2 secondes, la plus récente en dernier : la dernière minute
  history: number[];
  // Secondes restantes, quand on connaît le total en octets (le zip d'un moteur)
  remaining: number | null;
}

const WINDOW = 5;
export const HISTORY = 30;

// Ce qu'on garde d'un job pendant qu'il télécharge.
interface Track {
  phase: string;
  // Les octets reçus, relevés à chaque seconde
  samples: number[];
  history: number[];
  ticks: number;
}

// Les octets reçus par un job en cours, ou `null` s'il ne télécharge pas (en attente,
// en pause, ou en train d'écrire ses fichiers : le réseau ne sert plus).
function receivedBytes(job: QueueJob): number | null {
  const progress = job.progress;
  if (job.status.state !== "running" || !progress) return null;
  if (progress.phase === "bytes") return progress.done;
  if (progress.phase === "chunks") return progress.bytes;
  return null;
}

// Relève une seconde de plus et calcule la vitesse.
function measure(track: Track, bytes: number, job: QueueJob): Rate {
  track.samples.push(bytes);
  if (track.samples.length > WINDOW + 1) track.samples.shift();
  const { samples } = track;
  const seconds = samples.length - 1;
  const speed = seconds > 0 ? (samples[seconds] - samples[0]) / seconds : null;

  track.ticks += 1;
  if (speed !== null && track.ticks % 2 === 0) {
    track.history.push(speed);
    if (track.history.length > HISTORY) track.history.shift();
  }

  const progress = job.progress;
  const remaining =
    progress?.phase === "bytes" && speed
      ? (progress.total - progress.done) / speed
      : null;
  return { speed, history: [...track.history], remaining };
}

// La vitesse de chaque job qui télécharge, par id de job.
export function useTransferRates(jobs: QueueJob[]): Record<number, Rate> {
  const latest = useRef(jobs);
  const tracks = useRef(new Map<number, Track>());
  const [rates, setRates] = useState<Record<number, Rate>>({});

  useEffect(() => {
    latest.current = jobs;
  }, [jobs]);

  useEffect(() => {
    const timer = setInterval(() => {
      const next: Record<number, Rate> = {};
      for (const job of latest.current) {
        const bytes = receivedBytes(job);
        if (bytes === null) continue;
        const phase = job.progress!.phase;
        let track = tracks.current.get(job.id);
        // Nouveau job, nouvelle phase ou compteur reparti de zéro : on recommence.
        if (
          !track ||
          track.phase !== phase ||
          bytes < track.samples[track.samples.length - 1]
        ) {
          track = { phase, samples: [], history: [], ticks: 0 };
          tracks.current.set(job.id, track);
        }
        next[job.id] = measure(track, bytes, job);
      }
      // On oublie les jobs qui ne téléchargent plus.
      for (const id of tracks.current.keys()) {
        if (!(id in next)) tracks.current.delete(id);
      }
      // Rien ne télécharge, ni avant ni maintenant : pas de nouveau rendu.
      setRates((before) =>
        Object.keys(next).length === 0 && Object.keys(before).length === 0
          ? before
          : next,
      );
    }, 1000);
    return () => clearInterval(timer);
  }, []);

  return rates;
}
