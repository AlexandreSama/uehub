import type { Account, AssetKind } from "../api";

export type Filter = "all" | AssetKind;

// L'écran affiché sous la barre : la bibliothèque Fab ou les moteurs.
export type View = "library" | "engines";

const FILTERS: { id: Filter; label: string }[] = [
    { id: "all", label: "Tout" },
    { id: "pack", label: "Packs" },
    { id: "plugin", label: "Plugins" },
    { id: "project", label: "Projets" },
];

interface Props {
    view: View;
    onView: (view: View) => void;
    account: Account | null;
    count: number;
    query: string;
    onQuery: (query: string) => void;
    filter: Filter;
    onFilter: (filter: Filter) => void;
    onLogout: () => void;
}

export default function TopBar(props: Props) {
    const { view, onView, account, count, query, onQuery, filter, onFilter } =
        props;
    const { onLogout } = props;
    return (
        <header className="topbar">
            <span className="wordmark">uehub</span>
            <nav className="views" aria-label="Écran">
                <button
                    aria-current={view === "library" ? "page" : undefined}
                    onClick={() => onView("library")}
                >
                    Bibliothèque
                </button>
                <button
                    aria-current={view === "engines" ? "page" : undefined}
                    onClick={() => onView("engines")}
                >
                    Moteurs
                </button>
            </nav>
            {view === "library" && (
                <>
                    <input
                        className="search"
                        type="search"
                        value={query}
                        onChange={(e) => onQuery(e.target.value)}
                        placeholder={`Rechercher parmi tes ${count} assets`}
                        aria-label="Rechercher un asset"
                    />
                    <div className="filters" role="group" aria-label="Type d'asset">
                        {FILTERS.map((f) => (
                            <button
                                key={f.id}
                                aria-pressed={filter === f.id}
                                onClick={() => onFilter(f.id)}
                            >
                                {f.label}
                            </button>
                        ))}
                    </div>
                </>
            )}
            {account && (
                <div className="account">
          <span title={`Session rangée dans : ${account.storage}`}>
            {account.name}
          </span>
                    <button className="quiet" onClick={onLogout}>
                        Se déconnecter
                    </button>
                </div>
            )}
        </header>
    );
}