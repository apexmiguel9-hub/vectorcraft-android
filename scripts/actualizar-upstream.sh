#!/bin/sh
# Traer upstream sin romper el port.
#
# MEDIDO por que esto existe: upstream empuja cada 15 minutos (M8.18 a las 04:01,
# M8.19 a las 04:13). Un fork se queda atrás en minutos, y cuando se rebasea el
# conflicto cae en dos ficheros que el port no controla:
#
#   - `Cargo.lock`: upstream anade dependencias todos los dias, y nosotros
#     tambien (`android-activity`). Los dos tocan el mismo fichero y git no sabe
#     como mezclarlos.
#   - `Cargo.toml`: la linea `members` del workspace.
#
# La regla que se aplica: **nunca resolver conflictos de `Cargo.lock` a mano**. Se
# coge el del upstream y se vuelve a generar con el nuestro encima. Un lock
# resuelto a mano con "<<<<<<<" dentro compila a veces y falla otros dias, y el
# fallo sale como "noSuchMethod" en una dependencia que ayer funcionaba.
set -e
cd "$(dirname "$0")/.."
REPO="https://github.com/storytold/vectorcraft.git"

echo "==> trayendo upstream"
git fetch "$REPO" main:refs/remotes/upstream/main

DETRAS=$(git rev-list --count HEAD..upstream/main)
if [ "$DETRAS" -eq 0 ]; then
    echo "ya estamos al dia"
    exit 0
fi
echo "==> $DETRAS commits detras. Rebaseando."

git rebase upstream/main || {
    echo
    echo "==> rebase con conflictos."
    for f in Cargo.lock Cargo.toml; do
        if git diff --name-only --diff-filter=U | grep -qx "$f"; then
            echo "    $f: se toma el de upstream y se vuelve a generar"
            git checkout --theirs "$f"
            git add "$f"
        fi
    done
    if git diff --name-only --diff-filter=U | grep -q .; then
        echo "==> quedan conflictos que no son de lock. Miralos a mano:"
        git diff --name-only --diff-filter=U
        exit 1
    fi
    git rebase --continue
}

echo "==> regenerando Cargo.lock con nuestras dependencias encima"
env -u RUSTUP_HOME -u CARGO_HOME \
    PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
    /root/.cargo/bin/cargo generate-lockfile 2>&1 | tail -3

echo
echo "listo. Revisa con:"
echo "  git diff --stat Cargo.lock   # deberia ser solo anadir, no bajar versiones"
echo "  git push origin main"