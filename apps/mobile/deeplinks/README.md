# HTTPS invitation handoff (not deployed)

Keep `app.omochi.fyi` as the web dashboard. The future native handoff is
`https://mobile.omochi.fyi/open#g:PAYLOAD`, `#share:PAYLOAD`, or `#owner:PAYLOAD`.
The app already accepts these HTTPS invitation strings through paste/custom
handoff. OS automatic HTTPS delivery is not enabled by this change.

Before activation:

1. Confirm the Apple Team ID, distribution bundle ID and Android distribution
   certificate SHA-256 (including Play App Signing if used). Replace the example
   values. Never publish placeholders or associate development signing keys with
   the production domain.
2. Serve the completed JSON as `/.well-known/apple-app-site-association` and
   `/.well-known/assetlinks.json` on `mobile.omochi.fyi` over HTTPS without redirects.
   Merge the entitlement into the iOS target and provisioning profile, and merge
   the Android filter into MainActivity. Keep the existing custom-scheme filter.
3. Serve `/open` as a small static fallback page. Read and scrub the fragment
   before other modules, retain it only in memory, and offer explicit custom URI
   launch and a link back to `https://app.omochi.fyi/#TYPE:PAYLOAD`. Do not auto
   enroll, send the fragment to a server, put it in query/path, use analytics, or
   infer app installation with a timer.
4. Change the web handoff button's destination to the HTTPS URL for Android/iOS;
   retain custom URIs for desktop/development. The initial user gesture should
   navigate directly across domains; do not introduce a redirect chain.
5. On signed real devices, verify installed/uninstalled, cold/warm launch,
   disabled association, each invitation type, background/foreground and duplicate
   delivery. Check that returning to web preserves the invitation and never
   redeems it automatically. Android: `adb shell pm verify-app-links --re-verify
   fyi.omochi.mobile`, then `adb shell pm get-app-links fyi.omochi.mobile`.
   On iOS test from Notes and from the app.omochi.fyi button in Safari.

The root `/` and other web paths must remain outside the app association.
DNS, hosting and signed-device verification are a separate release step.
