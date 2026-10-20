// useEngines : quand la liste d'Epic est relue en arrière-plan, et quand elle ne l'est pas.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  api,
  type EngineCatalog,
  type LocalEngine,
  type QueueJob,
} from "./api";
import { useEngines } from "./engines";

// Les appels à Rust sont remplacés : le test décide de ce qu'ils répondent.
vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  api: {
    cachedEngines: vi.fn(),
    localEngines: vi.fn(),
    availableEngines: vi.fn(),
  },
}));

const HOUR = 60 * 60 * 1000;
const NOW = new Date("2026-10-19T12:00:00Z").getTime();
const NO_JOBS: QueueJob[] = [];

function catalog(ageHours: number): EngineCatalog {
  return {
    eulaAccepted: true,
    checkedAt: (NOW - ageHours * HOUR) / 1000,
    engines: [{ version: "5.8.4", size: 1, extracted: 2, date: "2026-10-14" }],
  };
}

const ENGINE: LocalEngine = {
  version: "5.8.3",
  dir: "/home/a/UnrealEngine/5.8.3",
  byUehub: true,
  update: "5.8.4",
  replacedBy: null,
};

// Laisse passer le temps, et les promesses qui attendent.
async function wait(ms: number) {
  await act(() => vi.advanceTimersByTimeAsync(ms));
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(NOW);
  vi.mocked(api.cachedEngines).mockResolvedValue(catalog(1));
  vi.mocked(api.localEngines).mockResolvedValue([ENGINE]);
  vi.mocked(api.availableEngines).mockResolvedValue(catalog(0));
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

describe("useEngines", () => {
  it("compte les mises à jour d'après la liste du disque, sans réseau", async () => {
    const { result } = renderHook(() => useEngines(true, NO_JOBS));
    await wait(0);
    expect(result.current.catalog).toEqual(catalog(1));
    expect(result.current.updates).toEqual([ENGINE]);
    // Une liste d'une heure : pas de relecture.
    expect(api.availableEngines).not.toHaveBeenCalled();
  });

  it("relit une liste de plus de quatre heures, une fois la bibliothèque là", async () => {
    vi.mocked(api.cachedEngines).mockResolvedValue(catalog(5));
    const { rerender } = renderHook(({ ready }) => useEngines(ready, NO_JOBS), {
      initialProps: { ready: false },
    });
    await wait(0);
    expect(api.availableEngines).not.toHaveBeenCalled();
    rerender({ ready: true });
    await wait(0);
    expect(api.availableEngines).toHaveBeenCalledTimes(1);
  });

  it("relit en arrière-plan quand la liste vieillit", async () => {
    renderHook(() => useEngines(true, NO_JOBS));
    await wait(0);
    // La liste avait une heure : trois heures plus tard, elle en a quatre.
    await wait(2 * HOUR);
    expect(api.availableEngines).not.toHaveBeenCalled();
    await wait(HOUR);
    expect(api.availableEngines).toHaveBeenCalledTimes(1);
  });

  it("ne relit rien sans moteur installé", async () => {
    vi.mocked(api.cachedEngines).mockResolvedValue(null);
    vi.mocked(api.localEngines).mockResolvedValue([]);
    renderHook(() => useEngines(true, NO_JOBS));
    await wait(5 * HOUR);
    expect(api.availableEngines).not.toHaveBeenCalled();
  });

  it("hors ligne, réessaie quatre heures plus tard, pas à chaque tour", async () => {
    vi.mocked(api.cachedEngines).mockResolvedValue(catalog(5));
    vi.mocked(api.availableEngines).mockRejectedValue("pas de réseau");
    const { result } = renderHook(() => useEngines(true, NO_JOBS));
    await wait(0);
    expect(api.availableEngines).toHaveBeenCalledTimes(1);
    expect(result.current.error).toBe("pas de réseau");
    await wait(3 * HOUR);
    expect(api.availableEngines).toHaveBeenCalledTimes(1);
    await wait(HOUR);
    expect(api.availableEngines).toHaveBeenCalledTimes(2);
  });
});
