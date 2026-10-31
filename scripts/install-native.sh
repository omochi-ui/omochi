#!/bin/sh
set -eu
source_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
destination=${1:-"$HOME/.local/bin"}
mkdir -p "$destination"
install -m 755 "$source_dir/bin/omochi-ui" "$destination/omochi-ui"
install -m 755 "$source_dir/bin/omochi-utils" "$destination/omochi-utils"
printf 'Installed omochi-ui and omochi-utils in %s\n' "$destination"
