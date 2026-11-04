# omochi-cli

Run the Omochi dashboard and connect applications built with the Python,
TypeScript, or Rust SDK, or with `omochi-utils`.

```sh
cargo install omochi-cli --locked
omochi-ui hub
```

Open **http://127.0.0.1:5555/**, create a dashboard, and choose **Add view** to
add a running app's surface. The hub starts a local gateway automatically.
SDK apps must use `UnixSocketServerTransport` for discovery. Keep the hub and
app running while using their UI.

The package installs `omochi-ui`, including the `hub` and `gateway` commands.
It is separate from the `omochi-ui` SDK crate. The dashboard assets are included;
Bun, Node.js, and a repository checkout are not needed to build the published crate.
Native services support Linux and macOS.

With Node.js 22+, you can also run `npx @omochi-ui/cli hub` without a global install.
For apps on another machine, see the [remote connection guide](https://omochi.fyi/en/docs/guides/remote/).
