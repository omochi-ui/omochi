import asyncio
import os
from dataclasses import dataclass
from typing import TypedDict

from omochi_ui import schema, Expr, InputSchema, ActionContext, UI, App, UnixSocketServerTransport


@dataclass
class State:
    temperature: float = 50
    result: str | None = None

@dataclass
class RunInput:
    temperature: float

class RunInputExpr(TypedDict):
    temperature: float | Expr[float]

RUN_INPUT = InputSchema(RunInput, RunInputExpr)
TEMPERATURE = RUN_INPUT.field("temperature", schema.number())



app = App(State())


@app.action("run", schema=RUN_INPUT)
async def run(input: RunInput, context: ActionContext[State]):
    async with context.update() as state:
        state.temperature = input.temperature
        state.result = f"Ran at temperature {input.temperature:g}"


@app.surface("main")
def main(ui: UI, state: State) -> None:
    form = ui.local("form", RunInput(state.temperature))
    ui.text("Hello", key="title", variant="h2")
    ui.slider("temperature", 0, 100, bind=form.field(TEMPERATURE).bind())
    ui.button("Run", run.call({"temperature": form.field(TEMPERATURE).value()}), variant="primary")
    if state.result:
        ui.text(state.result, key="result")


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
