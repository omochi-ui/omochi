# Docker

Run the native hub and the Python example in separate Linux containers. Docker
Engine on Linux and Docker Desktop on macOS/Windows are supported targets.
Published CLI images support `linux/amd64` and `linux/arm64`.

## Start from a checkout

Choose an existing `native-vX.Y.Z` release that includes Docker publication. Set
`OMOCHI_VERSION` to `X.Y.Z` (without `native-v`), then run from the repository root:

```sh
export OMOCHI_VERSION=X.Y.Z
docker compose -f deploy/docker/compose.yaml up --build -d --wait
```

In PowerShell use `$env:OMOCHI_VERSION = "X.Y.Z"` instead of `export`.
The hub is pulled from `ghcr.io/omochi-ui/omochi`; the Python example and SDK are
built from this checkout. Prefer the checkout matching the native release.
Open http://127.0.0.1:5555/, create a dashboard, add the **Python example** surface,
and save. Move the slider and press **Run** to send an action to Python.
Set `OMOCHI_PORT` to another host port if 5555 is occupied.

To build the hub from local sources instead (including before the first GHCR release):

```sh
export OMOCHI_VERSION=dev
docker compose -f deploy/docker/compose.yaml -f deploy/docker/compose.build.yaml up --build -d --wait
```

The initial build downloads Bun and Rust dependencies and compiles native binaries.
No Bun or Rust installation is required on the host. The runtime contains
`omochi-ui`, `omochi-utils`, CA certificates and curl for health checks.
`omochi-ui` is the entrypoint; pass CLI arguments after the image name, or use
`--entrypoint omochi-utils` to run utilities.

## Storage and lifecycle

The processes run as UID/GID 10001. Each container has a private, mode-0700
`/run/omochi` tmpfs. A named volume shares only the `python-example` service
socket and manifest. The hub's gateway socket is temporary and is recreated
on container restart. Docker volumes stay inside the Linux VM on Docker Desktop;
host filesystem socket forwarding is not needed.

The `config` volume preserves identities, trust and dashboards in
`/var/lib/omochi`. Python application state is in memory and resets on restart.
The initial volume ownership is populated from the image directories. Use the
provided named volumes rather than mounting an arbitrary host directory.

Use the same Compose file flags as at startup for subsequent commands:

```sh
docker compose -f deploy/docker/compose.yaml logs -f
docker compose -f deploy/docker/compose.yaml down
```

`down` retains data. Add `--volumes` only to reset identities, trust, dashboards
and service runtime data; deleted identities require pairing again. To upgrade,
back up the config volume, select the new `OMOCHI_VERSION`, pull the hub and run
`up --build -d --wait`. Restore the backup when rolling back incompatible state.

HTTP is unauthenticated and is published only on host loopback. Origin validation
remains enabled. Remote iroh access, external HTTP publication, host application
socket sharing and Docker orchestration through `omochi.toml` are outside this
example. The Docker socket is never mounted into the services.

## Verification

Build both images using the local override, then run:

```sh
bun scripts/smoke-docker.ts
```

The smoke test uses its own Compose project and host port 15555 (override with
`OMOCHI_PORT`). It verifies embedded UI assets, WebSocket actions, permissions,
Origin rejection, normal and forced restarts, identity and dashboard persistence.
It removes only its own project and volumes on exit. CI runs it natively on both
Linux architectures. For Docker Desktop acceptance, run the same test and manually
verify the slider/Run interaction in the browser on macOS and Windows.

## Publishing

The Docker workflow publishes on stable `native-vX.Y.Z` GitHub releases, after
validating the release tag against Cargo and testing both architectures. It can
also be dispatched with an existing tag on main; dry-run defaults to true.
Only the tested images are uploaded. The public tag is `X.Y.Z`; `X.Y.Z-amd64`
and `X.Y.Z-arm64` are the architecture tags used to assemble its manifest.
There is no `latest` tag. SDK releases do not publish native images.

The workflow uses `GITHUB_TOKEN` with `packages: write`. After the first publish,
a maintainer must make the GHCR package public in its package settings if it is
initially private. Confirm anonymous access with a fresh Docker config:

```sh
docker --config /tmp/omochi-anonymous-pull pull ghcr.io/omochi-ui/omochi:X.Y.Z
docker buildx imagetools inspect ghcr.io/omochi-ui/omochi:X.Y.Z
```

Confirm both Linux platforms are listed. A failed publish can be retried using
the same release tag. Releasing is separate from implementing/testing these files.
