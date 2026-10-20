// useAppUpdate : la recherche du démarrage, puis toutes les quatre heures.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, type AppInfo } from "./api";
import { useAppUpdate } from "./update";

vi.mock("./api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./api")>()),
  api: {
    appInfo: vi.fn(),
    checkUpdate: vi.fn(),
    installUpdate: vi.fn(),
  },
}));

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;

function info(pkg: AppInfo["package"]): AppInfo {
  return { version: "0.2.0", package: pkg, dev: false };
}

// Laisse passer le temps, et les promesses qui attendent. React n'affiche ce qui a changé
// qu'à la fin de chaque attente : le démarrage a la sienne, `wait(0)`.
async function wait(ms: number) {
  await act(() => vi.advanceTimersByTimeAsync(ms));
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(api.appInfo).mockResolvedValue(info("appimage"));
  vi.mocked(api.checkUpdate).mockResolvedValue(null);
});

afterEach(() => {
  vi.useRealTimers();
  vi.clearAllMocks();
});

it("cherche au démarrage, puis toutes les quatre heures", async () => {
  vi.mocked(api.checkUpdate)
    .mockResolvedValueOnce(null)
    .mockResolvedValueOnce({ version: "0.3.0", date: null, notes: null });
  const { result } = renderHook(() => useAppUpdate());
  await wait(0);
  expect(api.checkUpdate).toHaveBeenCalledTimes(1);
  expect(result.current.available).toBeNull();
  // Les tours de dix minutes ne cherchent pas avant quatre heures.
  await wait(3 * HOUR + 50 * MINUTE);
  expect(api.checkUpdate).toHaveBeenCalledTimes(1);
  await wait(10 * MINUTE);
  expect(api.checkUpdate).toHaveBeenCalledTimes(2);
  expect(result.current.available?.version).toBe("0.3.0");
});

it("ne cherche jamais sans paquet : c'est le gestionnaire de paquets", async () => {
  vi.mocked(api.appInfo).mockResolvedValue(info(null));
  renderHook(() => useAppUpdate());
  await wait(10 * HOUR);
  expect(api.checkUpdate).not.toHaveBeenCalled();
});

it("une recherche à la main repousse la suivante", async () => {
  const { result } = renderHook(() => useAppUpdate());
  await wait(0);
  await wait(3 * HOUR);
  await act(() => result.current.check());
  expect(api.checkUpdate).toHaveBeenCalledTimes(2);
  await wait(2 * HOUR);
  expect(api.checkUpdate).toHaveBeenCalledTimes(2);
  await wait(2 * HOUR);
  expect(api.checkUpdate).toHaveBeenCalledTimes(3);
});

it("garde ce qu'il sait quand le réseau manque, et ne cherche pas pendant l'installation", async () => {
  vi.mocked(api.checkUpdate)
    .mockResolvedValueOnce({ version: "0.3.0", date: null, notes: null })
    .mockRejectedValueOnce("pas de réseau");
  // L'installation ne finit jamais : uehub redémarrerait à la fin.
  vi.mocked(api.installUpdate).mockReturnValue(new Promise(() => {}));
  const { result } = renderHook(() => useAppUpdate());
  await wait(0);
  await wait(4 * HOUR);
  expect(api.checkUpdate).toHaveBeenCalledTimes(2);
  expect(result.current.available?.version).toBe("0.3.0");

  act(() => {
    void result.current.install();
  });
  await wait(10 * HOUR);
  expect(api.checkUpdate).toHaveBeenCalledTimes(2);
});
