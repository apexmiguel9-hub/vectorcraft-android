#!/bin/sh
# Traer upstream sin romper el port.
#
# MEDIDO por que esto existe: upstream empuja cada 15 minutos (M8.18 a las 04:01,
# M8.19 a las 04:13) y en una tarde paso de 97 a 188 commits. Un fork se queda atras en
# minutos, y el conflicto cae casi siempre en ficheros que el port no controla:
#
#   - `Cargo.lock`: upstream anade dependencias todos los dias, y nosotros
#     tambien (`android-activity`). Los dos tocan el mismo fichero y git no sabe
#     como mezclarlos.
#   - `Cargo.toml`: la linea `members` del workspace.
#
# MEDIDO, y aqui se cambio de rebase a **merge**, con numeros:
#
#   upstream 188 commits por delante, 315 ficheros que cambia, y el port toca 32.
#   `git merge-tree` en seco, que no toca la rama: **un solo conflicto**, en
#   `.gitignore`. Todo lo demas auto-mergea, incluido `crates/ui-egui/src/canvas.rs` con
#   sus 20 commits de upstream por encima.
#
# Con `rebase` eso serian **79 oportunidades de conflicto** en vez de una, y ademas un
# rebase **reescribe la historia ya publicada**: los SHA de los commits que hay en
# `origin/main` cambian todos, y en un fork eso desconecta a quien haya hecho clone.
#
# La regla que se aplica: **nunca resolver conflictos de `Cargo.lock` a mano**. Se coge el
# del upstream y se vuelve a generar con el nuestro encima. Un lock resuelto a mano con
# "<<<<<<<" dentro compila a veces y falla otros dias, y el fallo sale como "noSuchMethod"
# en una dependencia que ayer funcionaba.
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
echo "==> $DETRAS commits detras. Fusionando."

git merge upstream/main -m "Merge upstream/main: $DETRAS commits, y el port encima" || {
    echo
    echo "==> merge con conflictos."
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
    git commit --no-edit
}

echo "==> regenerando Cargo.lock con nuestras dependencias encima"
env -u RUSTUP_HOME -u CARGO_HOME \
    PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin" \
    /root/.cargo/bin/cargo generate-lockfile 2>&1 | tail -3

echo
echo "listo. Revisa con:"
echo "  git diff --stat Cargo.lock   # deberia ser solo anadir, no bajar versiones"
echo "  git push origin main"