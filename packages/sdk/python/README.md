# Omochi UI for Python

Attach browser UIs to Python programs with the declarative A2UI runtime.

```sh
pip install omochi-ui
# Optional WebSocket transport:
pip install 'omochi-ui[websocket]'
```

Requires Python 3.11 or newer. The CLI is distributed separately as
`@omochi-ui/cli` on npm (Node.js 22+ launcher), or as a native archive.

See the [SDK guide](https://github.com/omochi-ui/omochi/blob/main/docs/sdk.md)
and [project README](https://github.com/omochi-ui/omochi) for examples.

The next minor release introduces typed action inputs and requires migrating
action registration and local field references. See the SDK guide migration section.
Python wheels include `py.typed`; both mypy and Pyright are tested.

Licensed under Apache-2.0.
