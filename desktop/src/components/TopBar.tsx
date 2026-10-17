import type { Account, AssetKind } from "../api";

export type Filter = "all" | AssetKind;

const FILTERS: { id: Filter; label: string }[] = [
    { id: "all", label: "Tout" },
    { id: "pack", label: "Packs" },
    { id: "plugin", label: "Plugins" },
    { id: "project", label: "Projets" },
];

interface Props {
    account: Account | null;
    count: number;
    query: string;
    onQuery: (query: string) => void;
    filter: Filter;
    onFilter: (filter: Filter) => void;
    onLogout: () => void;
}

export default function TopBar(props: Props) {
    const { account, count, query, onQuery, filter, onFilter, onLogout } = props;
    return (
        <header className="topbar">
            <span className="wordmark">uehub</span>
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