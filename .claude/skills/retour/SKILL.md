---
name: retour
description: Traite les retours de test du porteur sur l'étape en cours de Vigie (bugs, remarques d'ergonomie, validations) et corrige avec des tests de non-régression.
argument-hint: [retours de test en texte libre]
disable-model-invocation: true
---

# Retours de test

Retours du porteur :

$ARGUMENTS

## Procédure
1. **Trier** chaque point et le reformuler en une ligne :
   - 🐞 Bug : comportement différent de l'attendu.
   - 🎨 Ergonomie : ça marche mais ça gêne.
   - 💡 Évolution : nouvelle demande hors de l'étape.
   - ✅ Validé : test réussi.
   Si un point est ambigu, pose une question précise au lieu de deviner.
2. **Évolutions** : ne les implémente pas dans l'étape en cours. Propose de les ajouter à `docs/ROADMAP.md` (étape existante ou nouvelle) et attends l'accord.
3. **Bugs** : pour chacun, dans l'ordre :
   - Trouve la cause racine, pas seulement le symptôme. Explique-la en une phrase.
   - Écris d'abord un **test qui reproduit le bug** quand c'est possible.
   - Corrige le code, puis vérifie que le test passe.
   - Fais un commit `fix(<zone>): …`.
   - Si le bug touche aux secrets, à la crypto, à l'IPC ou aux clés d'hôte, redélègue à `security-reviewer` sur le correctif.
4. **Ergonomie** : propose la modification (une phrase ou un petit schéma texte) si elle n'est pas évidente, puis applique-la.
5. Lance `npm run check`.
6. **Rends compte** sous forme de tableau : point → catégorie → cause → correction → test ajouté.
7. Donne un **mini plan de test** qui couvre uniquement les points corrigés, puis arrête-toi.

Quand le porteur indique que tout est validé : passe l'étape en ✅ dans `docs/ROADMAP.md`, mets à jour `CHANGELOG.md` si besoin, commite, puis **demande** s'il faut pousser la branche et ouvrir ou fusionner la PR vers `main`.
