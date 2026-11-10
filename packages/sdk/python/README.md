# Omochi UI for Python

Attach browser UIs to Python programs with the declarative A2UI runtime.

```sh
pip install omochi-ui
# Optional WebSocket transport:
pip install 'omochi-ui[websocket]'
```

Requires Python 3.11 or newer. The CLI is distributed separately as
`@omochi-ui/cli` on npm (Node.js 22+ launcher), or as a native archive.

See the [SDK guide](https://omochi.fyi/en/docs/guides/sdks/)
and [project README](https://github.com/omochi-ui/omochi) for examples.

```python
import asyncio
from omochi_ui import App, LocalServerTransport, ui

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
asyncio.run(app.start(LocalServerTransport("temperature", name="Temperature")))
```

Start `npx @omochi-ui/cli hub` separately, open **http://127.0.0.1:5555/**,
and add the `Temperature` Surface. Run the app and Hub as the same OS user with
the same runtime directory. Keep both running while using the UI.

State is shared by the app's connected clients; each renderer keeps its own draft
inputs. Action annotations provide input validation. See [typed inputs](https://omochi.fyi/en/docs/reference/typed-inputs/)
for forms, scopes, explicit schemas, and static typing, and [transports](https://omochi.fyi/en/docs/reference/transports/)
for other connection choices and limits.

Licensed under Apache-2.0.
