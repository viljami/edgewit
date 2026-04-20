#!/bin/sh
set -e

# Ensure /data exists and is owned by the current user (edgewit)
if [ ! -d /data ]; then
  mkdir -p "/$EDGEWIT_DATA_DIR/indexes"
  chown -R edgewit:edgewit "/$EDGEWIT_DATA_DIR" && chmod -R 0755 "/$EDGEWIT_DATA_DIR"
fi

exec runuser -u edgewit -- "$@"
