// La pastille de Moteurs : le nombre à l'écran, et la phrase pour un lecteur d'écran.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import type { AppUpdate } from "../update";
import Rail from "./Rail";

const update: AppUpdate = {
  app: null,
  available: null,
  checking: false,
  progress: null,
  message: null,
  error: null,
  check: vi.fn(),
  install: vi.fn(),
};

// Chaque test part d'une page vide.
afterEach(cleanup);

function rail(engineUpdates: number) {
  return render(
    <Rail
      view="library"
      onView={() => {}}
      account={null}
      onLogout={() => {}}
      update={update}
      queueBusy={false}
      engineUpdates={engineUpdates}
    />,
  );
}

it("affiche le nombre de moteurs à mettre à jour sur Moteurs", () => {
  rail(2);
  const button = screen.getByRole("button", { name: /Moteurs/ });
  expect(button.textContent).toBe("Moteurs2 mises à jour");
});

it("accorde la phrase, et n'affiche rien sans mise à jour", () => {
  const { unmount } = rail(1);
  expect(screen.getByRole("button", { name: /Moteurs/ }).textContent).toBe(
    "Moteurs1 mise à jour",
  );
  unmount();
  rail(0);
  expect(screen.getByRole("button", { name: /Moteurs/ }).textContent).toBe(
    "Moteurs",
  );
});
