#!/bin/sh
# Prépare les comptes de test puis lance sshd au premier plan.
# - TEST_PASSWORD (obligatoire) : mot de passe de testpwd, généré à la volée par l'appelant.
# - /keys/authorized_keys (monté en lecture seule) : clés publiques autorisées pour testkey.
set -eu

if [ -z "${TEST_PASSWORD:-}" ]; then
    echo "TEST_PASSWORD manquant" >&2
    exit 1
fi

id testpwd >/dev/null 2>&1 || useradd -m -s /bin/bash testpwd
id testkey >/dev/null 2>&1 || useradd -m -s /bin/bash testkey
printf 'testpwd:%s\n' "$TEST_PASSWORD" | chpasswd
# Le compte testkey n'a pas de mot de passe utilisable.
usermod -p '*' testkey
unset TEST_PASSWORD

install -d -m 700 -o testkey -g testkey /home/testkey/.ssh
if [ -f /keys/authorized_keys ]; then
    install -m 600 -o testkey -g testkey /keys/authorized_keys /home/testkey/.ssh/authorized_keys
fi

# Nouvelles clés d'hôte à chaque démarrage du conteneur.
ssh-keygen -A >/dev/null
exec /usr/sbin/sshd -D -e
