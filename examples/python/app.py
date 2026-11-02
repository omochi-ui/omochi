import asyncio
import os
from dataclasses import dataclass

from omochi_ui import ActionContext, App, UnixSocketServerTransport, ui


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
    return UnixSocketServerTransport(service_id, name="Python example", icon="🐍")


if __name__ == "__main__":
    asyncio.run(app.start(transport()))
