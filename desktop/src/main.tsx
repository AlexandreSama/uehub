import React from "react";
import ReactDOM from "react-dom/client";
// Police embarquée dans l'app : rien n'est chargé depuis Internet. Archivo est variable
// en graisse et en largeur : les titres et les petits libellés utilisent sa forme étroite.
import "@fontsource-variable/archivo/wdth.css";
import "./styles.css";
import { api, loadHome } from "./api";
import { setLang, type Lang } from "./i18n";
import App from "./App";
import Crash from "./components/Crash";

// Une erreur JavaScript que personne n'attrape finit dans le journal, avec sa pile.
function report(error: unknown) {
  const text =
    error instanceof Error
      ? `${error.message}\n${error.stack ?? ""}`
      : String(error);
  api.logWebview(text).catch(() => {});
}
window.addEventListener("error", (event) =>
  report(event.error ?? event.message),
);
window.addEventListener("unhandledrejection", (event) => report(event.reason));

// La langue, puis le dossier personnel (les chemins s'affichent avec « ~ ») : le premier
// affichage est déjà dans la bonne langue. Hors de Tauri, celle du navigateur.
async function start() {
  const fallback: Lang = navigator.language.startsWith("fr") ? "fr" : "en";
  setLang(await api.language().catch(() => fallback));
  await loadHome();
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Crash onError={report}>
        <App />
      </Crash>
    </React.StrictMode>,
  );
}

void start();
