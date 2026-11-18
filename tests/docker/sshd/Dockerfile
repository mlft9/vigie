# Serveur OpenSSH jetable pour les tests d'intégration de core-ssh (CI et poste local).
# Aucun secret dans l'image : le mot de passe et les clés autorisées sont fournis au
# démarrage du conteneur (voir entrypoint.sh et le job « integration » de ci.yml).
# Image de base épinglée par empreinte (chaîne d'approvisionnement) ; Dependabot ne la suit
# pas encore : la mettre à jour avec la version de Debian.
FROM debian:12-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc

# openssh-sftp-server : sous-système SFTP (étape 4), paquet séparé sous Debian 12 et seulement
# « recommandé » par openssh-server.
RUN apt-get update \
    && apt-get install -y --no-install-recommends openssh-server openssh-sftp-server procps \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /run/sshd \
    && rm -f /etc/ssh/ssh_host_*

COPY sshd_config /etc/ssh/sshd_config.d/younji-tests.conf
COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod 0755 /usr/local/bin/entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
