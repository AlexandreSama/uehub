import React from "react";
import ReactDOM from "react-dom/client";
// Police embarquée dans l'app : rien n'est chargé depuis Internet. Archivo est variable
// en graisse et en largeur : les titres et les petits libellés utilisent sa forme étroite.
import "@fontsource-variable/archivo/wdth.css";
import "./styles.css";
import { loadHome } from "./api";
import App from "./App";

// Le dossier personnel d'abord : les chemins s'affichent ensuite avec « ~ ».
void loadHome().then(() =>
    ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
        <React.StrictMode>
            <App />
        </React.StrictMode>,
    ),
);