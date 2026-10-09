#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == build-host ]] || { echo 'S5 verification requires Build-host' >&2; exit 1; }
name="aster-odcs-s5-pg-$RANDOM-$$"
cleanup() { podman rm -fv "$name" >/dev/null; }
trap cleanup EXIT
podman run -d --name "$name" -e POSTGRES_PASSWORD=disposable-test -p 127.0.0.1::5432 docker.io/library/postgres:17-alpine >/dev/null
ready=false
for _ in $(seq 1 60); do
  if podman exec "$name" pg_isready -h 127.0.0.1 -U postgres >/dev/null 2>&1; then ready=true; break; fi
  sleep 1
done
$ready || { echo 'S5 PostgreSQL not ready' >&2; exit 1; }
port=$(podman port "$name" 5432/tcp | cut -d: -f2)
export ASTER_TEST_METADATA_URL="postgres://postgres:disposable-test@127.0.0.1:$port/postgres"
"$@"
cleanup
trap - EXIT
if podman container exists "$name"; then echo 'S5 PostgreSQL cleanup failed' >&2; exit 1; fi
echo 'odcs-s5-postgres OK'
