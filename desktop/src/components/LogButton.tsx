// « Afficher le journal » : montre uehub.log dans le gestionnaire de fichiers. Le bouton
// suit chaque message d'erreur, pour qu'on trouve le détail au moment où on en a besoin.
import { useState } from "react";
import { api } from "../api";
import { t } from "../i18n";

export default function LogButton({
  className = "quiet",
}: {
  className?: string;
}) {
  const [error, setError] = useState<string | null>(null);

  async function show() {
    setError(null);
    try {
      await api.showLog();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <button className={className} onClick={() => void show()}>
        {t("log.show")}
      </button>
      {error && (
        <span className="error" role="alert">
          {error}
        </span>
      )}
    </>
  );
}
