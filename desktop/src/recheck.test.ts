// useTick : un tour toutes les dix minutes, et au retour de la fenêtre.
import { renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useTick } from "./recheck";

const MINUTE = 60 * 1000;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

it("fait un tour toutes les dix minutes, et quand la fenêtre revient", () => {
  const onTick = vi.fn();
  const { unmount } = renderHook(() => useTick(onTick));
  vi.advanceTimersByTime(9 * MINUTE);
  expect(onTick).not.toHaveBeenCalled();
  vi.advanceTimersByTime(MINUTE);
  expect(onTick).toHaveBeenCalledTimes(1);
  window.dispatchEvent(new Event("focus"));
  expect(onTick).toHaveBeenCalledTimes(2);

  // Démonté : plus rien.
  unmount();
  vi.advanceTimersByTime(60 * MINUTE);
  window.dispatchEvent(new Event("focus"));
  expect(onTick).toHaveBeenCalledTimes(2);
});

it("appelle la dernière version de la fonction", () => {
  const first = vi.fn();
  const second = vi.fn();
  const { rerender } = renderHook(({ fn }) => useTick(fn), {
    initialProps: { fn: first },
  });
  rerender({ fn: second });
  window.dispatchEvent(new Event("focus"));
  expect(first).not.toHaveBeenCalled();
  expect(second).toHaveBeenCalledTimes(1);
});
