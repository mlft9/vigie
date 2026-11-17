---
name: release
description: Prépare une version de Vigie (numéro de version synchronisé, CHANGELOG, tag) pour déclencher la compilation des installeurs en CI.
argument-hint: [version, ex. 0.2.0]
disable-model-invocation: true
---

# Préparer la version $0

1. Vérifie qu'on est sur `main`, à jour, avec un arbre de travail propre, et que `npm run check` passe.
2. Vérifie que `$0` respecte SemVer et qu'elle est supérieure à la version actuelle. Tant qu'on n'a pas publié de 1.0, on reste en `0.x`.
3. Mets le même numéro de version dans :
   - `package.json` (et `package-lock.json` via `npm version $0 --no-git-tag-version`)
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - les `Cargo.toml` des crates internes, si elles sont versionnées
4. Dans `CHANGELOG.md`, renomme `[Non publié]` en `[$0] - <date du jour AAAA-MM-JJ>` et ajoute une nouvelle section `[Non publié]` vide au-dessus.
5. Commite : `chore(release): version $0`.
6. Montre le résumé : versions modifiées, extrait du CHANGELOG.
7. **Demande l'accord** avant de lancer `git tag -a v$0 -m "Version $0"` puis `git push origin main --follow-tags`. Le tag déclenche `build.yml`.
8. Une fois le push fait, rappelle au porteur où trouver les installeurs : onglet Actions, puis le run du tag, puis Artifacts. Après publication, les installeurs seront aussi dans les Releases.

Ne touche jamais aux clés de signature : elles vivent uniquement dans les secrets GitHub.
