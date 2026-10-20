// Les textes : la bonne forme selon le nombre, les variables, et la langue.
import { render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { lang, setLang, t, tx, type Key } from "./i18n";

afterEach(() => setLang("en"));

it("remplace les variables et choisit singulier ou pluriel", () => {
  setLang("fr");
  expect(lang()).toBe("fr");
  expect(document.documentElement.lang).toBe("fr");
  // Comme côté Rust : 0 et 1 au singulier en français, seulement 1 en anglais.
  expect(t("summary.files", { n: 0, size: "1 Ko" })).toBe("0 fichier, 1 Ko");
  expect(t("summary.files", { n: 2, size: "1 Ko" })).toBe("2 fichiers, 1 Ko");
  setLang("en");
  expect(t("summary.files", { n: 0, size: "1 kB" })).toBe("0 files, 1 kB");
  expect(t("summary.files", { n: 1, size: "1 kB" })).toBe("1 file, 1 kB");
});

it("écrit les nombres à la façon de la langue", () => {
  setLang("fr");
  // Intl sépare les milliers par une espace fine insécable en français.
  expect(t("library.count", { n: 1234 })).toBe("1 234 assets Fab");
  setLang("en");
  expect(t("library.count", { n: 1234 })).toBe("1,234 Fab assets");
});

it("laisse une variable manquante visible, et une clé inconnue aussi", () => {
  expect(t("errors.offline")).toBe(
    "No connection: can't reach {host}. Check your Internet access.",
  );
  expect(t("nope.missing" as Key)).toBe("nope.missing");
});

it("place un élément au milieu d'une phrase", () => {
  setLang("fr");
  const { container } = render(
    <p>{tx("login.step_paste", { code: <code>authorizationCode</code> })}</p>,
  );
  expect(container.innerHTML).toBe(
    "<p>Colle la valeur de <code>authorizationCode</code> affichée ensuite.</p>",
  );
});
