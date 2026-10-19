// Écrit latest.json, le fichier que lit la mise à jour intégrée de uehub, à partir des
// paquets signés que vient de fabriquer `npm run tauri build`.
//
// Usage, depuis desktop/ : node scripts/latest-json.mjs TON_COMPTE/uehub "Ce qui change"
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const [repo, notes = ""] = process.argv.slice(2);
if (!repo) {
    console.error(
        'Usage : node scripts/latest-json.mjs TON_COMPTE/uehub "notes"',
    );
    process.exit(1);
}

// La version de l'app est celle du workspace, dans ../Cargo.toml.
const cargo = readFileSync("../Cargo.toml", "utf8");
const version = cargo.match(/^version = "(.+)"$/m)[1];

// Le dossier de chaque paquet, et la clé sous laquelle l'updater le cherche :
// {os}-{arch}-{paquet}.
const bundle = "../target/release/bundle";
const formats = [
    ["deb", ".deb", "linux-x86_64-deb"],
    ["rpm", ".rpm", "linux-x86_64-rpm"],
    ["appimage", ".AppImage", "linux-x86_64-appimage"],
];

const platforms = {};
for (const [dir, extension, key] of formats) {
    const folder = join(bundle, dir);
    const file = existsSync(folder)
        ? readdirSync(folder).find(
            (f) => f.endsWith(extension) && f.includes(version),
        )
        : undefined;
    if (!file || !existsSync(join(folder, `${file}.sig`))) {
        console.warn(`Pas de ${extension} signé pour ${version} : ignoré.`);
        continue;
    }
    platforms[key] = {
        signature: readFileSync(join(folder, `${file}.sig`), "utf8"),
        url: `https://github.com/${repo}/releases/download/v${version}/${file}`,
    };
}

if (Object.keys(platforms).length === 0) {
    console.error("Aucun paquet signé : lance d'abord npm run tauri build.");
    process.exit(1);
}

const latest = {
    version,
    notes,
    pub_date: new Date().toISOString(),
    platforms,
};
writeFileSync(
    join(bundle, "latest.json"),
    `${JSON.stringify(latest, null, 2)}\n`,
);
console.log(`${join(bundle, "latest.json")} : uehub ${version}`);