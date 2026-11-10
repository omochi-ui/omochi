#!/bin/sh
# Register an already-extracted, complete bundle in the current desktop account.
set -eu
BUNDLE=$(cd "${1:?Usage: register_linux.sh /path/to/bundle}" && pwd)
test -x "$BUNDLE/mobile"
case "$BUNDLE" in
*%* | *'
'*)
  printf '%s\n' 'Move the bundle to a path without percent signs or newlines before registering links.' >&2
  exit 1
  ;;
esac
# The desktop-entry parser unescapes before the Exec argument parser. Keep
# quotes, backslashes, percent expansions, and shell-reserved characters literal.
EXECUTABLE=$(printf '%s' "$BUNDLE/mobile" | sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/`/\\\\`/g; s/\$/\\\\$/g; s/%/%%/g')
APPLICATIONS="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
mkdir -p "$APPLICATIONS"
cat >"$APPLICATIONS/fyi.omochi.mobile.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Omochi
Exec="$EXECUTABLE" %u
Terminal=false
StartupWMClass=fyi.omochi.mobile
Categories=Network;
MimeType=x-scheme-handler/omochi;
DESKTOP
xdg-mime default fyi.omochi.mobile.desktop x-scheme-handler/omochi
printf '%s\n' 'Omochi invitation links registered for the current user.'
