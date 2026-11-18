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
- Connexion SSH rapide : hôte, port, utilisateur, puis mot de passe ou clé privée (Ed25519, ECDSA, RSA, avec ou sans passphrase). La clé se choisit dans la boîte de dialogue de fichiers du système.
- Terminal complet dans chaque onglet (couleurs, Unicode, vim, htop, tmux), avec redimensionnement répercuté sur le serveur et historique de 10 000 lignes.
- Plusieurs sessions en onglets : ouvrir, fermer, renommer (double-clic ou F2), réordonner par glisser-déposer ou au clavier. Un onglet masqué garde son contenu.
- Recherche dans le terminal, copier/coller (Ctrl+Maj+C / Ctrl+Maj+V, Maj+Inser, menu du clic droit) et liens web ouverts par Ctrl+clic dans le navigateur.
- Raccourcis clavier en Ctrl+Maj, pour laisser au shell les siens (Ctrl+C, Ctrl+R, Ctrl+W…). Leur liste est affichée dans les Paramètres.
- Détection des coupures réseau : l'onglet passe à « déconnecté » et propose de se reconnecter, avec le formulaire pré-rempli (le mot de passe ou la passphrase est redemandé).
- L'interface reste réactive même quand le serveur envoie un très gros volume de texte (`cat` d'un gros fichier, `yes`).
- Terminal basé sur xterm.js 5.5 : la version 6 empêchait l'application de démarrer avec le durcissement `freezePrototype` (fenêtre blanche) ; un test le vérifie désormais (ADR-025).
- Tests d'intégration contre un vrai serveur OpenSSH, lancés en conteneur à chaque push et utilisables en local (`npm run test:integration`).
- Coffre chiffré : au premier lancement, création avec un mot de passe maître (12 caractères au moins), un indicateur de robustesse en direct et un avertissement clair : un mot de passe oublié ne peut pas être récupéré. Aux lancements suivants, l'application s'ouvre verrouillée.
- Verrouillage manuel (bouton de la barre latérale ou Ctrl+Maj+L) et automatique après une période d'inactivité réglable (1, 5, 10, 15, 30 ou 60 minutes ; 15 par défaut). La mise en veille de l'ordinateur compte comme de l'inactivité.
- Changement du mot de passe maître depuis les Paramètres.
- Carnet d'hôtes : nom, adresse, port, utilisateur, identité, groupe, étiquettes et notes. Classement par groupes, recherche instantanée, double-clic pour se connecter.
- Identités réutilisables (utilisateur, avec mot de passe, clé ou les deux), partagées entre plusieurs hôtes. Une connexion ou une reconnexion depuis le carnet ne redemande rien. Si l'identité a une clé et un mot de passe, la clé est essayée d'abord.
- Section Clés : import d'une clé privée existante dans le coffre (sa passphrase n'est demandée qu'une fois), renommage, copie de la clé publique.
- Gestion des clés d'hôte connues : liste et suppression après un avertissement. L'alerte de clé modifiée y mène directement.
- Taille de police du terminal réglable dans les Paramètres.

### Modifié
- Toute l'application, connexion rapide comprise, n'est accessible qu'après le déverrouillage du coffre.
- Verrouiller l'application, ou recharger l'interface, ferme toutes les sessions SSH et efface le contenu des terminaux (ADR-026).
- Les clés d'hôte acceptées sont désormais conservées dans le coffre, et non plus dans un fichier `known_hosts` en clair. Celles déjà acceptées sont reprises automatiquement à la création du coffre (ADR-027).
- Les préférences (langue, thème, police) sont enregistrées par l'application dans un fichier `settings.json` ; les réglages déjà faits sont repris automatiquement.

### Sécurité
- Configuration Tauri durcie : politique de sécurité du contenu (CSP) stricte, aucun contenu distant, pas d'API Tauri globale exposée à l'interface, prototypes JavaScript gelés.
- Chaque commande du cœur doit être autorisée explicitement pour la fenêtre (capabilities) ; les appels de l'interface passent en plus par une iframe d'isolation qui filtre les commandes (ADR-015).
- La fenêtre ne peut afficher que le contenu de l'application : la navigation vers une autre adresse et l'ouverture de nouvelles fenêtres sont refusées.
- Tests automatiques qui échouent si la configuration de sécurité est affaiblie.
- Vérification des clés d'hôte SSH : à la première connexion, l'empreinte `SHA256:…` est affichée et doit être acceptée. Si la clé d'un serveur connu change, même pour un autre type de clé, la connexion est bloquée avec une alerte, sans moyen de passer outre. Les clés acceptées sont conservées dans un fichier `known_hosts` propre à Younji (ADR-023).
- Algorithmes SSH modernes uniquement : échange de clés hybride post-quantique en priorité, ni SHA-1, ni CBC, ni compression (ADR-019).
- Les mots de passe et passphrases ne sont transmis qu'une fois au cœur de l'application, détruits dès la fin de l'authentification (une copie interne à la bibliothèque SSH ne peut pas être effacée, limite documentée), et jamais renvoyés à l'interface ni écrits dans les journaux. L'interface ne connaît jamais le chemin ni le contenu de la clé privée (ADR-020, ADR-021).
- Collage protégé : un texte de plusieurs lignes ou contenant des caractères invisibles demande une confirmation avec aperçu, et ne peut pas sortir du mode « collage » du shell.
- Le serveur ne peut pas lire le presse-papiers par une séquence d'échappement. Un lien dont le texte affiché peut différer de l'adresse réelle demande une confirmation qui montre cette adresse ; seuls les liens `http` et `https` s'ouvrent (ADR-022).
- Les messages d'accueil du serveur sont affichés comme du texte simple, sans séquences d'échappement, et leur taille est limitée.
- La CSP admet désormais les balises de style intégrées dont le terminal a besoin, sans autoriser les attributs de style ni aucune source distante (ADR-018).
- Recharger l'interface ferme proprement les sessions ouvertes ; F5 et Ctrl+R sont désactivés hors du terminal pour éviter une fermeture accidentelle.
- Coffre chiffré par Argon2id et XChaCha20-Poly1305 : le fichier ne contient aucune donnée lisible et toute modification est détectée. Un mauvais mot de passe et un fichier abîmé donnent le même message, sans indice sur la cause (ADR-005).
- Après 3 mots de passe maître erronés, chaque nouvel essai est ralenti, jusqu'à 30 secondes, y compris la vérification du mot de passe actuel lors d'un changement (ADR-028).
- Le verrouillage automatique est décidé par le cœur de l'application, pas par l'interface. Son délai est stocké dans le coffre : il ne peut pas être modifié en dehors de l'application (ADR-028).
- La robustesse du mot de passe maître est vérifiée par le cœur de l'application, pas seulement par l'indicateur affiché (ADR-029).
- Les mots de passe des identités et les clés privées ne sont jamais renvoyés à l'interface : seules la présence d'un mot de passe, la clé publique et l'empreinte sont affichées (ADR-030).
- Les clés d'hôte, chiffrées et authentifiées dans le coffre, ne peuvent plus être lues ni falsifiées par un autre logiciel (ADR-027).
- Après un changement de mot de passe maître, la copie de sauvegarde du coffre est remplacée par une copie du nouveau coffre, ou supprimée : l'ancien mot de passe n'ouvre plus aucune copie gérée par Younji.
- Le coffre est écrit de façon à ne jamais être laissé à moitié écrit, avec une copie de sauvegarde de la version précédente ; sa création n'écrase jamais un coffre existant ni une copie de sauvegarde restée seule.
