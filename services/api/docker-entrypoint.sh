#!/bin/sh
# Runs as root (the container's default user — see the Dockerfile) so it can
# fix ownership on the mounted upload volume, then drops to the unprivileged
# `camp` user before exec'ing the real process.
#
# Why this is needed: a freshly attached Railway volume is mounted owned by
# root regardless of which user the image otherwise runs as, so the app
# (running as `camp`) gets EACCES the first time it tries to write an
# upload. Only the mount root needs fixing — every file the app creates
# afterward is already owned by `camp`, since setpriv below is what starts
# the app in the first place.
set -e

: "${UPLOAD_DIR:=/data/uploads}"
mkdir -p "$UPLOAD_DIR"
chown camp:camp "$UPLOAD_DIR"

exec setpriv --reuid=camp --regid=camp --init-groups "$@"
