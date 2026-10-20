// Les petites fonctions d'affichage de api.ts : ce qu'elles écrivent est ce que tu lis.
import { describe, expect, it } from "vitest";
import {
  branchOf,
  compareVersions,
  engineRange,
  formatBytes,
  formatDay,
  formatDuration,
  jobFor,
  progressText,
  removalText,
  updateText,
  type JobStatus,
  type QueueJob,
} from "./api";

// Un job d'installation de moteur, dans l'état voulu.
function engineJob(version: string, status: JobStatus): QueueJob {
  return {
    id: 1,
    request: { kind: "installEngine", version, folder: "/home/a/UnrealEngine" },
    title: `Unreal Engine ${version}`,
    thumbnail: null,
    target: `/home/a/UnrealEngine/${version}`,
    status,
    artifactId: null,
    finishedAt: null,
    progress: null,
  };
}

describe("versions", () => {
  it("compare les nombres, pas le texte", () => {
    expect(compareVersions("5.10", "5.9")).toBeGreaterThan(0);
    expect(compareVersions("5.4", "5.4.0")).toBe(0);
    expect(["5.10", "4.27", "5.9"].sort(compareVersions)).toEqual([
      "4.27",
      "5.9",
      "5.10",
    ]);
  });

  it("donne la branche et l'intervalle", () => {
    expect(branchOf("5.8.3")).toBe("5.8");
    expect(engineRange(["4.27", "5.0", "5.4"])).toBe("4.27 – 5.4");
    expect(engineRange(["5.4"])).toBe("5.4");
    expect(engineRange([])).toBe("Version inconnue");
  });
});

describe("tailles, durées et dates", () => {
  it("écrit les tailles à la française", () => {
    expect(formatBytes(1_234_000_000)).toBe("1,2 Go");
    expect(formatBytes(39_818_576_005)).toBe("39,8 Go");
    expect(formatBytes(1_572_864)).toBe("1,6 Mo");
    expect(formatBytes(999)).toBe("1 Ko");
  });

  it("arrondit les durées à la minute", () => {
    expect(formatDuration(30)).toBe("moins d'une minute");
    expect(formatDuration(1500)).toBe("25 min");
    expect(formatDuration(3600)).toBe("1 h");
    expect(formatDuration(4300)).toBe("1 h 12 min");
  });

  it("écrit le jour d'une date d'Epic", () => {
    expect(formatDay("2026-09-22")).toBe("22 sept. 2026");
    expect(formatDay(null)).toBe("");
  });

  it("décrit l'avancement", () => {
    expect(progressText(null)).toBe("Préparation…");
    expect(
      progressText({ phase: "chunks", done: 33, total: 112, bytes: 9 }),
    ).toBe("Morceaux téléchargés 33 / 112");
    expect(
      progressText({
        phase: "bytes",
        done: 12_300_000_000,
        total: 39_800_000_000,
      }),
    ).toBe("Téléchargé 12,3 Go / 39,8 Go");
  });
});

describe("la file", () => {
  it("retrouve l'installation d'une version tant qu'elle n'est pas finie", () => {
    const waiting = engineJob("5.8.4", { state: "waiting" });
    expect(jobFor([waiting], "5.8.4")).toBe(waiting);
    expect(jobFor([waiting], "5.8.3")).toBeUndefined();
    const failed = engineJob("5.8.4", { state: "failed", error: "réseau" });
    expect(jobFor([failed], "5.8.4")).toBe(failed);
    const done = engineJob("5.8.4", {
      state: "done",
      summary: { files: 1, bytes: 1, dir: "/", note: null },
    });
    expect(jobFor([done], "5.8.4")).toBeUndefined();
  });
});

describe("les phrases des assets", () => {
  it("dit ce qu'apporte une mise à jour", () => {
    expect(updateText({ kind: "newBuild" }, "5.3")).toBe(
      "Le vendeur a publié une nouvelle build de la version pour 5.3.",
    );
    expect(updateText({ kind: "closerEngine", engine: "5.4" }, "5.3")).toBe(
      "Une version pour UE 5.4 existe : elle remplacera celle pour 5.3.",
    );
  });

  it("accorde le bilan d'un retrait", () => {
    expect(removalText({ deleted: 48, shared: 0, missing: 0 })).toBe(
      "48 fichiers supprimés.",
    );
    expect(removalText({ deleted: 1, shared: 1, missing: 1 })).toBe(
      "1 fichier supprimé. 1 fichier gardé : un autre asset du projet l'utilise aussi. 1 était déjà absent.",
    );
    expect(removalText({ deleted: 0, shared: 3, missing: 2 })).toBe(
      "0 fichier supprimé. 3 fichiers gardés : un autre asset du projet les utilise aussi. 2 étaient déjà absents.",
    );
  });
});
