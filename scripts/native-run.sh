#!/bin/sh
set -eu
root=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
binary=$1
shift
profile=${OMOCHI_NATIVE_PROFILE:-debug}
case "$profile" in debug|release) ;; *) printf 'OMOCHI_NATIVE_PROFILE must be debug or release\n' >&2; exit 2;; esac
target="$root/packages/server-rust/target/$profile/$binary"
if [ ! -x "$target" ]; then
  printf 'Native binary missing. Run bun run build:native from the checkout.\n' >&2
  exit 1
fi
exec "$target" "$@"
