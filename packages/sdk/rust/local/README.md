# Omochi local IPC

The shared, protocol-independent implementation of private local publication for
Rust, Python, Node and Bun. It owns runtime format 2, service locks, manifests,
UDS/named-pipe listeners and byte streams. The SDKs own framing, snapshots and
application messages. Node-API and C ABI adapters use the same lifetime registry.

See the repository's `spec/local-service-ownership.md` for the contract and
`spec/local-service.schema.json` for the manifest schema. This crate follows the
SDK release version, independently of native CLI releases.
