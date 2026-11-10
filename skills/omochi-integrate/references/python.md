# Python integration

Use Python 3.11+. `App` accepts a state object; `@app.action` infers the input
schema from the first parameter's dataclass annotation. `app.mount(main)` mounts
the render function; its `ui` declarations are valid only while rendering.

This standalone entry point supports both the stdio preview and a local hub.
Replace the result calculation in `run` with the program's existing operation.
Keep slow work outside the `context.update()` block. If the program already has
an asyncio loop, await the runtime within that lifecycle instead of nesting
`asyncio.run()`.

```python
import asyncio
import os
from dataclasses import dataclass

from omochi_ui import ActionContext, App, LocalServerTransport, ui


@dataclass
class State:
    temperature: float = 50
    result: str | None = None

@dataclass
class RunInput:
    temperature: float

app = App(State())


@app.action
async def run(input: RunInput, context: ActionContext[State]):
    async with context.update() as state:
        state.temperature = input.temperature
        state.result = f"Ran at temperature {input.temperature:g}"


def main(state: State) -> None:
    form = ui.local("form", RunInput(state.temperature))
    ui.text("Hello", key="title", variant="h2")
    ui.slider("temperature", 0, 100, bind=form.temperature)
    ui.button("Run", on_click=run, input=form, variant="primary")
    if state.result:
        ui.text(state.result, key="result")


app.mount(main)


def transport():
    """Publishes on a discoverable unix socket when OMOCHI_SERVICE names a service id.

    Without it the app speaks JSONL on stdio, which is what `omochi-ui dev` expects.
    """
    service_id = os.environ.get("OMOCHI_SERVICE")
    if not service_id:
        return None
    return LocalServerTransport(service_id, name="Python example", icon="🐍")


if __name__ == "__main__":
    asyncio.run(app.start(transport()))
```

For the local hub, run `OMOCHI_SERVICE=my-tool uv run ui_app.py` from a project
with the SDK installed. Without the variable, launch this program under
`omochi-ui dev --` as described in [setup](setup.md).

Use `print(..., file=sys.stderr)` or stderr logging when using stdio.

API and further controls: [SDK guide](https://omochi.fyi/en/docs/guides/sdks/).
