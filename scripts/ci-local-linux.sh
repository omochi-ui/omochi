#!/usr/bin/env bash
set -euo pipefail
export PATH=/opt/python/cp311-cp311/bin:$PATH
export CARGO_HOME=/tmp/omochi-cargo RUSTUP_HOME=/tmp/omochi-rustup BUN_INSTALL=/tmp/omochi-bun
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
export PATH="$CARGO_HOME/bin:$BUN_INSTALL/bin:$PATH"
python -m pip install build hatchling auditwheel typing_extensions
python scripts/build-local.py --release
python -m build --wheel --outdir /tmp/omochi-wheel packages/sdk/python
mkdir -p artifacts/local-python
auditwheel repair --wheel-dir artifacts/local-python /tmp/omochi-wheel/*.whl
cargo test --manifest-path packages/sdk/rust/Cargo.toml -p omochi-local
if [[ "${OMOCHI_LIBC}" == musl ]]; then
  curl -fsSL https://bun.com/install | bash -s -- "bun-v$(cat .bun-version)"
  python scripts/test-local-adapters.py --runtime bun
fi
