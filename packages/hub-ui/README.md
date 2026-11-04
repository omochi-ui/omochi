# Dashboard localization

Both the local hub dashboard and the hosted dashboard use [i18next](https://www.i18next.com/)
with [react-i18next](https://react.i18next.com/) and its browser language detector.
Resources are bundled so translation does not require a network connection.

The saved language (`omochi.dashboard.language` in localStorage) takes precedence over
the browser's preferred languages. Regional variants such as `ja-JP` resolve to `ja`;
unsupported languages and missing translations fall back to English. The language
selector switches the current screen without reloading or discarding editor state.
The document's `lang` and `dir` follow the selected language.

To add a language:

1. Add `src/locales/<language>.ts`, matching the English resource structure with
   `satisfies LocaleResource`. Keep interpolation variables and rich text tags intact.
2. Import and register its resources in `src/i18n.ts`, and add its code and native
   name to `languages` in that file.
3. Extend the resource parity test in `test/i18n.test.ts` to include the new locale.
   Add plural forms required by that language using i18next's plural suffixes.

Use semantic translation keys and `useTranslation()` in React components. Translate
complete messages, including counts and interpolated names, rather than concatenating
sentence fragments. Use `Trans` for rich text. Retain error codes in state and translate
them at the UI boundary; transient feedback also stores message keys to follow language
changes. Dashboard names, app names, view content, and source-provided diagnostics remain
as supplied by their authors. Newly created default names use the current language and
then behave as user data.

Validate with `bun run --filter @omochi-ui/hub-ui test`,
`bun run --filter @omochi-ui/hub-ui build`, and
`bun run --filter @omochi-ui/hub-ui build:hosted`.
After building native binaries, `bun packages/hub-ui/test/browser.mjs` checks the live
dashboard, including detection, switching, persistence, editor state, and narrow layouts.

Gateway registration stays attached to its original hub, owner grant and browser
identity across disconnects. Reconnect with that authority to check the job or
retry saving; enrollment is never replayed automatically. Dismissing an invitation
releases the browser prompt without cancelling work already started on the hub.
`bun tests/browser/gateway-reconnect.mjs` checks this lifecycle with real React
components and Chromium, controlled session replies and poll timers. Screenshots
are saved in `/tmp/omochi-gateway-reconnect` (or `SCREENSHOT_DIR`).
