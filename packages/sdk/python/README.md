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

```python
import asyncio
from omochi_ui import App, ui

app = App({"result": ""})


@app.action
async def run(temperature: float, context):
    async with context.update() as state:
        state["result"] = f"Ran at temperature {temperature:g}"


def main(state):
    temperature = ui.local("temperature", 50.0)
    with ui.column():
        ui.slider("Temperature", 0, 100, bind=temperature)
        ui.button("Run", on_click=run, input=temperature)
        ui.text(state["result"])


app.mount(main)
asyncio.run(app.start())
```

State is an ordinary Python object shared by the service's connected clients.
Each renderer keeps its own draft inputs. Input annotations generate validation;
actions receive concrete values when the user clicks. `app.start()` speaks JSONL
on stdio; connect it with `omochi-ui dev -- python app.py` to open a browser UI.

Use dataclasses for structured inputs, `type=` for TypedDict/container locals,
and `with ui.scope(key):` to give repeated components stable keys. Typed field
projections and explicit schemas are available when stronger checking is needed.
Existing `(ui, state)` Surface functions and explicit schema/field APIs continue
to work. `on_click=` accepts registered Action handles rather than raw callbacks.

The next minor release introduces typed action inputs and requires migrating
action registration and local field references. See the SDK guide migration section.
Python wheels include `py.typed`; both mypy and Pyright are tested.

Licensed under Apache-2.0.
