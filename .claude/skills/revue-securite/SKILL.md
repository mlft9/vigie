---
name: revue-securite
description: Lance une revue de sécurité ciblée de Younji (diff en cours, un dossier ou un sujet précis) avec l'agent security-reviewer, puis propose les corrections.
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
4. Sans attendre d'accord, corrige les problèmes **Critique**, **Élevée** et **Moyenne** (ceux avec lesquels tu es d'accord), chacun dans un commit `fix(security): …` avec un test quand c'est possible. Pour les **Faible** et les points « à discuter », corrige si l'effort est faible, sinon justifie dans le compte rendu.
5. Lance `npm run check`, relance `security-reviewer` sur les correctifs, puis rends compte : problème → décision → commit.
