# Vendored dependency patches

`netdev` 0.46.3 (MIT, upstream https://github.com/shellrow/netdev) is pinned by netwatch. Its Darwin IPv6 flags implementation refers to `libc::in6_ifreq`, which libc exposes only on macOS. The local patch retains the upstream macOS behavior and returns unavailable/default per-address flags on iOS, as netdev does for unsupported platforms. Address enumeration still uses getifaddrs. Remove the patch when upstream supports iOS without this macOS-only type. All other source files are unchanged.

`netwatch` 0.19.3 (MIT/Apache-2.0, upstream https://github.com/n0-computer/net-tools) routes iOS through macOS-only libc routing structures. The patch gives iOS address enumeration through netdev/getifaddrs and a five-second cancellable foreground monitor. Default-route and home-router metadata remain unavailable. Other platforms are unchanged. App suspension destroys the endpoint/monitor and resume re-enumerates addresses.
