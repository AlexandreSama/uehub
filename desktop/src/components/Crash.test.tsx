// Un composant qui plante : l'écran d'erreur à la place d'une fenêtre vide, et l'erreur
// envoyée au journal.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { setLang } from "../i18n";
import Crash from "./Crash";

vi.mock("../api", async (original) => ({
  ...(await original<typeof import("../api")>()),
  api: { showLog: vi.fn(() => Promise.resolve()) },
}));

afterEach(cleanup);

function Broken(): never {
  throw new Error("plus de moteur");
}

it("montre l'erreur, l'envoie au journal et ouvre le journal", () => {
  setLang("fr");
  // React écrit l'erreur dans la console : on la fait taire pour ce test.
  const quiet = vi.spyOn(console, "error").mockImplementation(() => {});
  const onError = vi.fn();
  render(
    <Crash onError={onError}>
      <Broken />
    </Crash>,
  );
  quiet.mockRestore();

  expect(screen.getByRole("alert").textContent).toContain(
    "Quelque chose a cassé",
  );
  expect(screen.getByText("plus de moteur")).toBeTruthy();
  // Le message et la pile des composants partent au journal.
  expect(onError).toHaveBeenCalledOnce();
  expect(String(onError.mock.calls[0][0])).toContain("plus de moteur");
  expect(String(onError.mock.calls[0][0])).toContain("Broken");

  fireEvent.click(screen.getByRole("button", { name: "Afficher le journal" }));
  expect(api.showLog).toHaveBeenCalledOnce();
});
