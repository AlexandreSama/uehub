// Le rail à gauche : les deux écrans, et tout en bas la mise à jour et le compte.
import { useEffect, useRef, useState } from "react";
import { api, formatDay, tilde, type Account } from "../api";
import { t } from "../i18n";
import { progressLabel, type AppUpdate } from "../update";
import { EngineIcon, LibraryIcon, UpdateIcon } from "./Common";
import LogButton from "./LogButton";

// L'écran affiché à droite du rail : la bibliothèque Fab ou les moteurs.
export type View = "library" | "engines";

interface Props {
  view: View;
  onView: (view: View) => void;
  account: Account | null;
  onLogout: () => void;
  update: AppUpdate;
  // Des téléchargements tournent : le redémarrage les arrêtera
  queueBusy: boolean;
  // Les moteurs installés dont Epic propose une version plus récente
  engineUpdates: number;
}

export default function Rail(props: Props) {
  const { view, onView, account, onLogout, update, queueBusy, engineUpdates } =
    props;
  return (
    <nav className="rail" aria-label={t("rail.screens")}>
      <span className="wordmark">uehub</span>
      <button
        className="rail-button"
        aria-current={view === "library" ? "page" : undefined}
        onClick={() => onView("library")}
      >
        <LibraryIcon />
        {t("screens.library")}
      </button>
      <button
        className="rail-button"
        aria-current={view === "engines" ? "page" : undefined}
        onClick={() => onView("engines")}
      >
        <EngineIcon />
        {t("screens.engines")}
        {engineUpdates > 0 && (
          <span className="rail-count">
            {engineUpdates}
            <span className="sr-only">
              {" "}
              {t("rail.updates", { n: engineUpdates })}
            </span>
          </span>
        )}
      </button>
      <div className="rail-foot">
        {update.available && (
          <UpdateMenu update={update} queueBusy={queueBusy} />
        )}
        {account && (
          <AccountMenu account={account} onLogout={onLogout} update={update} />
        )}
      </div>
    </nav>
  );
}

// Un menu qui s'ouvre au clic, et se referme sur un clic ailleurs ou sur Échap.
function usePopover() {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const outside = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const escape = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("pointerdown", outside);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", outside);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);

  return { open, setOpen, box };
}

// Le bouton ambre qui annonce une nouvelle version ; un clic ouvre ce qu'elle apporte.
function UpdateMenu({
  update,
  queueBusy,
}: {
  update: AppUpdate;
  queueBusy: boolean;
}) {
  const { open, setOpen, box } = usePopover();
  const { app, available, progress, error, install } = update;
  if (!app || !available) return null;
  // Un .deb ou un .rpm s'installe avec les droits d'administrateur.
  const asksPassword = app.package === "deb" || app.package === "rpm";

  return (
    <div className="update" ref={box}>
      {open && (
        <div className="update-menu" id="update-menu">
          <strong>{t("rail.available", { version: available.version })}</strong>
          <span className="muted">
            {available.date &&
              `${t("common.published_on", { date: formatDay(available.date) })}. `}
            {t("rail.you_have", { version: app.version })}
          </span>
          {available.notes && <p className="notes">{available.notes}</p>}
          {queueBusy && <p className="muted">{t("rail.queue_busy")}</p>}
          {asksPassword && <p className="muted">{t("rail.password")}</p>}
          <button
            className="primary"
            disabled={progress !== null}
            onClick={() => void install()}
          >
            {progress ? progressLabel(progress) : t("rail.install_restart")}
          </button>
          {error && (
            <>
              <p className="error" role="alert">
                {error}
              </p>
              <LogButton />
            </>
          )}
        </div>
      )}
      <button
        className="rail-button update-button"
        aria-expanded={open}
        aria-controls="update-menu"
        onClick={() => setOpen(!open)}
      >
        <UpdateIcon />
        {t("common.update")}
      </button>
    </div>
  );
}

// L'initiale du compte ; un clic ouvre son nom, où est rangée la session, la déconnexion,
// la version de uehub et le journal.
function AccountMenu({
  account,
  onLogout,
  update,
}: {
  account: Account;
  onLogout: () => void;
  update: AppUpdate;
}) {
  const { open, setOpen, box } = usePopover();
  const { app, checking, message, check } = update;
  // Le chemin du journal, lu à la première ouverture du menu
  const [logFile, setLogFile] = useState<string | null>(null);
  useEffect(() => {
    if (open && logFile === null) api.logFile().then(setLogFile, () => {});
  }, [open, logFile]);

  const name = account.name || t("rail.epic_account");
  return (
    <div className="account" ref={box}>
      {open && (
        <div className="account-menu" id="account-menu">
          <strong>{name}</strong>
          <span className="muted">
            {t("account.session_in", { storage: tilde(account.storage) })}
          </span>
          <button onClick={onLogout}>{t("rail.sign_out")}</button>
          {app && (
            <div className="app-version">
              <span>uehub {app.version}</span>
              {app.dev ? (
                <span className="muted">{t("rail.dev_build")}</span>
              ) : !app.package ? (
                <span className="muted">{t("rail.package_manager")}</span>
              ) : (
                <>
                  <button disabled={checking} onClick={() => void check()}>
                    {checking ? t("rail.checking") : t("rail.check")}
                  </button>
                  {message && <span className="muted">{message}</span>}
                </>
              )}
            </div>
          )}
          <div className="app-version">
            <span>{t("log.title")}</span>
            {logFile && <span className="muted path">{tilde(logFile)}</span>}
            <span className="muted">{t("log.hint")}</span>
            <LogButton className="" />
          </div>
        </div>
      )}
      <button
        className="avatar"
        aria-label={t("rail.account", { name })}
        aria-expanded={open}
        aria-controls="account-menu"
        onClick={() => setOpen(!open)}
      >
        {name.charAt(0).toUpperCase()}
      </button>
    </div>
  );
}
