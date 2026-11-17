---
paths:
  - ".github/**"
  - "deny.toml"
---

# Règles CI / chaîne d'approvisionnement

- Le dépôt est **public** depuis l'étape 0 (ADR-016) : tout ce qui est poussé (historique, journaux de CI, artefacts) est visible de tous. Par sobriété, les tests rapides tournent sous Linux à chaque push, et `build.yml` (Windows + Linux) uniquement en manuel (`workflow_dispatch`) ou sur tag `v*`.
- Des PR peuvent venir de forks : aucun secret ne doit être accessible à un workflow déclenché par `pull_request`.
- Utilise le cache (`Swatinem/rust-cache`, cache npm) pour réduire les durées.
- **Actions épinglées par SHA complet**, avec la version en commentaire (`uses: actions/checkout@<sha> # v4.x.x`). Dependabot les met à jour.
- `permissions:` explicites et minimales au niveau du workflow (`contents: read` par défaut), élargies seulement dans le job qui en a besoin.
- Jamais de `pull_request_target` avec un checkout du code de la PR.
- Secrets uniquement via `${{ secrets.X }}`, jamais affichés, jamais passés en argument de ligne de commande visible, jamais écrits dans un artefact.
- Linux : compiler sur `ubuntu-22.04` pour que les paquets restent compatibles avec les glibc plus anciennes.
- `ci.yml` doit inclure : fmt, clippy `-D warnings`, `cargo test`, `cargo deny check` (licences compatibles GPL-3.0, avis RustSec, sources limitées à crates.io), lint/types/tests du front, `npm audit --audit-level=high`, détection de secrets (gitleaks).
- `deny.toml` : liste blanche de licences (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0, MPL-2.0, GPL-3.0…). Toute exception est commentée.
