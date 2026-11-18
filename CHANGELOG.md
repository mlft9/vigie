# Journal des modifications

Toutes les évolutions notables du projet sont consignées ici.
Format inspiré de [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/), versions selon [SemVer](https://semver.org/lang/fr/).

## [Non publié]

### Ajouté
- Cadrage du projet : cahier des charges, feuille de route, architecture, règles de sécurité, journal des décisions, environnement de test.
- Configuration de Claude Code : instructions du projet, agents (sécurité, QA, documentation), commandes `/etape`, `/retour`, `/release` et `/revue-securite`.
- Choix du nom définitif du projet : **Vigie** (ADR-014), puis changement pour **Younji** (ADR-017), « Vigie » étant déjà une marque déposée pour des logiciels.
- Première version de l'application de bureau (Tauri 2 + React) pour Windows et Linux.
- Mise en page de base : barre latérale (Hôtes, Clés, Snippets, Tunnels, SFTP, Paramètres) et zone d'onglets. Seuls les Paramètres sont fonctionnels pour l'instant.
- Interface en français et en anglais : langue du système détectée au démarrage, changement de langue à chaud depuis les Paramètres.
- Thème clair ou sombre, qui suit le système par défaut et peut être forcé.
- Affichage du nom et de la version de l'application dans les Paramètres.
- Journaux de diagnostic dans le dossier de l'application, conservés 7 jours, sans aucune donnée sensible. Leur niveau de détail se règle avec la variable `YOUNJI_LOG`.
- Outillage de développement : une seule commande (`npm run check`) vérifie le formatage, le lint, les types et les tests ; `npm run audit` contrôle les vulnérabilités et les licences des dépendances.
- Intégration continue : vérifications à chaque push (tests, audit des dépendances, détection de secrets) et compilation à la demande des installeurs Windows (NSIS, MSI) et Linux (deb, AppImage), téléchargeables directement depuis la page du run, sans archive .zip.
- Mises à jour hebdomadaires des dépendances par Dependabot, avec un délai de 7 jours avant d'adopter une nouvelle version.
- Feuille de route : phase 2 « Younji, le client qui protège » (étapes 14 à 22), issue du tri de 36 idées d'audit, de durcissement et de confort (28 retenues).
- Dépôt public sous licence GPL-3.0 (fichier `LICENSE`), avec une politique de signalement des vulnérabilités (`SECURITY.md`) (ADR-016).

### Sécurité
- Configuration Tauri durcie : politique de sécurité du contenu (CSP) stricte, aucun contenu distant, pas d'API Tauri globale exposée à l'interface, prototypes JavaScript gelés.
- Chaque commande du cœur doit être autorisée explicitement pour la fenêtre (capabilities) ; les appels de l'interface passent en plus par une iframe d'isolation qui filtre les commandes (ADR-015).
- La fenêtre ne peut afficher que le contenu de l'application : la navigation vers une autre adresse et l'ouverture de nouvelles fenêtres sont refusées.
- Tests automatiques qui échouent si la configuration de sécurité est affaiblie.
