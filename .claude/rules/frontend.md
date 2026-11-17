---
paths:
  - "src/**/*.ts"
  - "src/**/*.tsx"
  - "src/**/*.css"
  - "src/locales/**"
---

# Règles frontend (React + TypeScript)

## Architecture
- Composants fonctionnels et hooks. Un composant par fichier, nommé en PascalCase.
- Dossiers : `src/components/` (UI réutilisable, dont `ui/` pour shadcn), `src/features/<domaine>/` (hosts, terminal, sftp, vault…), `src/stores/` (zustand), `src/lib/` (utilitaires, `ipc.ts`), `src/locales/`.
- **Tous les appels au backend passent par `src/lib/ipc.ts`**, avec des fonctions typées (types partagés dans `src/lib/types.ts`, alignés sur les structures Rust). Pas d'`invoke()` direct dans les composants.
- État global dans des stores zustand par domaine. L'état local reste dans le composant.

## Terminal (xterm.js)
- Une instance `Terminal` par session, conservée montée quand l'onglet est caché (masquage CSS) pour ne pas perdre le tampon.
- Redimensionnement avec l'addon fit + un `ResizeObserver`, puis envoi des colonnes/lignes au backend avec un anti-rebond.
- Écriture de la sortie par lots. Libère proprement (`dispose`) à la fermeture de l'onglet.

## Secrets côté interface
- Un champ mot de passe ou passphrase est vidé dès l'envoi au backend. Ne le garde jamais dans un store.
- Pas de `localStorage` / `sessionStorage` pour des données sensibles (hôtes compris : ils sont dans le coffre).
- Pas de `dangerouslySetInnerHTML`, ni de rendu HTML de contenu venant d'un serveur.

## i18n
- Toute chaîne visible passe par `t('domaine.cle')`. Ajoute la clé dans `fr.json` **et** `en.json` dans le même commit.
- Les erreurs du backend arrivent sous forme de code : affiche ``t(`errors.${code}`)`` avec un texte générique de repli.

## Style et accessibilité
- Tailwind + composants shadcn/ui. Pas de CSS en ligne, sauf pour des valeurs dynamiques.
- Navigation au clavier possible partout, focus visible, `aria-label` sur les boutons qui n'ont qu'une icône.
- Thème : variables CSS de shadcn, clair et sombre.

## Tests
- Vitest + Testing Library pour les composants et les stores qui ont de la logique. L'IPC est simulée via un mock de `src/lib/ipc.ts`.
