// Si un composant plante pendant l'affichage, React efface toute la fenêtre. On montre à
// la place ce qui s'est passé et où le retrouver, et l'erreur part dans le journal.
import { Component, type ErrorInfo, type ReactNode } from "react";
import { t } from "../i18n";
import LogButton from "./LogButton";

interface Props {
  children: ReactNode;
  onError: (error: unknown) => void;
}

interface State {
  error: Error | null;
}

export default class Crash extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  // La pile des composants dit où, dans l'interface, l'erreur a eu lieu.
  componentDidCatch(error: Error, info: ErrorInfo) {
    this.props.onError(
      `${error.message}\n${error.stack ?? ""}\n${info.componentStack ?? ""}`,
    );
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    return (
      <main className="login crash">
        <section className="node" role="alert">
          <header className="node-head">
            <span className="exec" aria-hidden="true" />
            <h1>{t("crash.title")}</h1>
          </header>
          <div className="node-body">
            <p>{t("crash.text")}</p>
            <p className="error">{error.message}</p>
            <div className="buttons">
              <LogButton className="primary" />
              <button onClick={() => window.location.reload()}>
                {t("crash.reload")}
              </button>
            </div>
          </div>
        </section>
      </main>
    );
  }
}
