---
paths:
  - "src-tauri/tauri.conf.json"
  - "src-tauri/capabilities/**"
  - "src-tauri/Cargo.toml"
  - "index.html"
---

# Règles de configuration Tauri (durcissement)

Toute modification de ces fichiers doit être justifiée dans le commit et soumise à `security-reviewer`.

- **CSP** stricte dans `tauri.conf.json` (`app.security.csp`) : `default-src 'self'`, pas de `unsafe-eval`, pas de source distante. `unsafe-inline` est toléré uniquement pour `style-src` si shadcn/Tailwind l'exige, et ce choix est documenté. `connect-src` limité à `ipc:` et `http://ipc.localhost`.
- **Aucun chargement d'URL distante** dans une fenêtre. Les liens externes s'ouvrent dans le navigateur du système via le plugin `opener`, avec une liste blanche de schémas (`https:` uniquement).
- `app.withGlobalTauri` reste à `false`.
- **Capabilities** : un fichier par fenêtre, avec la liste explicite des permissions nécessaires. Pas de `*:default` sur un plugin sensible sans le justifier. Pas de permission `shell:allow-execute` / `shell:allow-spawn`. Les processus externes (mstsc, xfreerdp, shells locaux) sont lancés par le backend Rust avec des arguments construits de façon sûre, jamais depuis l'interface.
- `fs` : pas d'accès générique au système de fichiers depuis l'interface. Les sélections de fichiers passent par le plugin `dialog`, et le backend valide le chemin reçu.
- Le bundle identifier et le nom de produit viennent d'un seul endroit (voir ADR-014). Ne les dupliquez pas.
- Plugin updater (étape 13) : clé publique dans la config, clé privée **uniquement** dans les secrets GitHub.
