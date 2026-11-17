---
name: revue-securite
description: Lance une revue de sécurité ciblée de Vigie (diff en cours, un dossier ou un sujet précis) avec l'agent security-reviewer, puis propose les corrections.
argument-hint: [cible optionnelle : chemin, sujet ou "tout"]
disable-model-invocation: true
---

# Revue de sécurité

Cible demandée : $ARGUMENTS

1. Détermine le périmètre :
   - Aucune cible : le diff de la branche courante par rapport à `main`, non commité compris.
   - Un chemin : ce fichier ou ce dossier en entier.
   - Un sujet (ex. « coffre », « IPC », « SFTP », « CI ») : les fichiers concernés, que tu identifies d'abord.
   - « tout » : l'ensemble du dépôt, crate par crate.
2. Délègue à l'agent `security-reviewer` en lui donnant le périmètre précis.
3. Présente son rapport tel quel, puis ajoute pour chaque problème ton avis : d'accord / à discuter, avec l'effort de correction estimé.
4. **Demande** quels problèmes corriger avant de modifier quoi que ce soit. Corrige ensuite chaque problème retenu dans un commit `fix(security): …` avec un test quand c'est possible.
