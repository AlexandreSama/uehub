// Les textes de l'interface, dans la langue que Rust a choisie d'après celle du système.
// Ce sont les mêmes fichiers que côté Rust : locales/en.json et locales/fr.json.
import { createElement, Fragment, type ReactNode } from "react";
import en from "../../locales/en.json";
import fr from "../../locales/fr.json";

export type Lang = "en" | "fr";

// fr.json doit avoir tous les textes d'en.json, sous la même forme : sinon, tsc refuse
// cette ligne.
const TEXTS: Record<Lang, typeof en> = { en, fr };

// "errors.offline", "library.count"… : toutes les clés d'en.json. Une faute de frappe dans
// t("…") est une erreur de compilation. Un texte à deux formes ({one, other}) est une clé.
type Keys<T> = {
  [K in keyof T & string]: T[K] extends string
    ? K
    : T[K] extends { other: string }
      ? K
      : `${K}.${Keys<T[K]>}`;
}[keyof T & string];
export type Key = Keys<typeof en>;

let current: Lang = "en";

// Appelée une fois, avant le premier affichage.
export function setLang(lang: Lang) {
  current = lang;
  document.documentElement.lang = lang;
}

export function lang(): Lang {
  return current;
}

type Vars = Record<string, string | number>;

// Le texte brut de la clé : la forme au singulier ou au pluriel selon `n`, comme côté Rust
// (le français met 0 et 1 au singulier, l'anglais seulement 1).
function template(key: Key, n: string | number | undefined): string {
  let node: unknown = TEXTS[current];
  for (const part of key.split(".")) {
    node = (node as Record<string, unknown> | undefined)?.[part];
  }
  if (typeof node === "string") return node;
  if (node && typeof node === "object") {
    const forms = node as { one: string; other: string };
    const form = new Intl.PluralRules(current).select(Number(n ?? 0));
    return form === "one" ? forms.one : forms.other;
  }
  return key;
}

// Les nombres s'écrivent à la façon de la langue : 1 234 en français, 1,234 en anglais.
function show(value: string | number): string {
  return typeof value === "number"
    ? new Intl.NumberFormat(current).format(value)
    : value;
}

// Le texte de `key`, ses {variables} remplacées : t("errors.offline", { host }).
export function t(key: Key, vars: Vars = {}): string {
  return template(key, vars.n).replace(/\{(\w+)\}/g, (all, name: string) =>
    name in vars ? show(vars[name]) : all,
  );
}

// Pareil, quand une variable est un élément : tx("login.step_paste", { code: <code>…</code> }).
export function tx(key: Key, vars: Record<string, ReactNode>): ReactNode {
  const parts = template(key, undefined).split(/\{(\w+)\}/);
  // Les morceaux impairs sont des noms de variables.
  return parts.map((part, i) =>
    createElement(
      Fragment,
      { key: i },
      i % 2 === 1 ? (vars[part] ?? `{${part}}`) : part,
    ),
  );
}
