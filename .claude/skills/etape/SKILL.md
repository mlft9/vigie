---
name: etape
description: Démarre ou reprend une étape de la feuille de route de Younji (docs/ROADMAP.md) en suivant le processus du projet, du plan jusqu'au plan de test manuel.
argument-hint: [numéro-étape]
disable-model-invocation: true
---

# Étape $0 de la feuille de route

## 1. Comprendre
- Lis l'étape $0 dans `docs/ROADMAP.md` : contenu et critères d'acceptation.
- Lis les exigences du `docs/CAHIER_DES_CHARGES.md` citées par l'étape, les ADR utiles de `docs/DECISIONS.md` et `docs/SECURITE.md`.
- Si l'étape a déjà commencé (une branche `etape/$0-*` existe), lis `git log main..HEAD` et reprends là où le travail s'est arrêté au lieu de recommencer.

## 2. Préparer
- Vérifie que l'arbre de travail est propre (`git status`). Sinon, demande quoi faire.
- Vérifie que les étapes précédentes sont ✅ dans `docs/ROADMAP.md`. Sinon, signale-le et demande s'il faut continuer quand même.
- Crée la branche `etape/$0-<slug-court>` depuis `main`.
- Passe l'étape en 🟨 dans `docs/ROADMAP.md`.

## 3. Planifier (en mode plan, puis attendre la validation)
Présente :
- Les sous-tâches, dans l'ordre, chacune livrable en un ou quelques commits.
- Les fichiers et crates touchés.
- Les **dépendances à ajouter**, avec leur rôle, leur licence et la vérification de leur dernière version.
- Les **risques de sécurité** propres à l'étape et la parade prévue.
- Les tests prévus (unitaires, intégration, interface).
- Les **questions ouvertes** pour le porteur, sous forme de choix avec ta recommandation.
N'écris aucun code avant que le porteur ait validé le plan.

## 4. Implémenter
- Avance sous-tâche par sous-tâche, avec un commit Conventional Commits par unité cohérente.
- Ajoute les tests en même temps que le code.
- Ajoute toutes les chaînes d'interface en FR et en EN.
- Lance `npm run check` régulièrement, et obligatoirement avant le dernier commit.
- Si une décision structurante apparaît en cours de route, demande l'avis du porteur et note-la pour `docs-keeper`.

## 5. Contrôler
1. Délègue à l'agent `security-reviewer` (diff `main...HEAD`). Corrige les problèmes **Critique** et **Élevée**. Pour les autres, corrige-les ou justifie-les.
2. Délègue à l'agent `qa-validator` avec le numéro d'étape $0.
3. Délègue à l'agent `docs-keeper`.
4. Commite les corrections et la documentation.

## 6. Livrer au porteur, puis s'arrêter
Présente, en français et de façon concise :
- Ce qui a été fait (5 à 8 puces au maximum) et les choix de sécurité notables.
- Comment tester : `npm run tauri dev`, ou lancement manuel du workflow `build.yml` sur la branche pour obtenir un installeur. **Demande avant de pousser la branche.**
- Le **plan de test manuel** de `qa-validator`, avec ses cases à cocher.
- Les limites connues et ce qui est reporté.
Ensuite, arrête-toi. Les retours arriveront avec `/retour`. Ne fusionne pas dans `main` sans l'accord explicite du porteur.
