// Ce que uehub revérifie de lui-même quand il reste ouvert, parfois des jours pendant
// un gros téléchargement : sa propre version, et la liste des moteurs d'Epic.
import { useEffect, useRef } from "react";

// Une vérification de plus de quatre heures est à refaire.
export const RECHECK_MS = 4 * 60 * 60 * 1000;

// Une minuterie ne compte pas le temps passé en veille : réglée sur quatre heures, elle
// pourrait attendre des jours sur un portable qu'on referme souvent. On regarde donc
// l'heure toutes les dix minutes, et quand la fenêtre revient au premier plan ; chacun
// compare à sa dernière vérification.
const TICK_MS = 10 * 60 * 1000;

export function useTick(onTick: () => void) {
    // La dernière version de la fonction, avec l'état du moment
    const latest = useRef(onTick);
    useEffect(() => {
        latest.current = onTick;
    });

    useEffect(() => {
        const tick = () => latest.current();
        const timer = setInterval(tick, TICK_MS);
        window.addEventListener("focus", tick);
        return () => {
            clearInterval(timer);
            window.removeEventListener("focus", tick);
        };
    }, []);
}