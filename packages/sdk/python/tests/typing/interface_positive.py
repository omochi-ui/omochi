from dataclasses import dataclass
from typing import NotRequired, TypedDict, assert_type

from omochi_ui import Action, ActionContext, App, Local, Ref, schema, ui


@dataclass
class State:
    result: str = ""


@dataclass
class Form:
    temperature: float = 50.0


class DictionaryForm(TypedDict):
    temperature: float
    note: NotRequired[str]


app = App(State())


@app.action
async def run(value: Form, ctx: ActionContext[State]) -> None:
    async with ctx.update() as current:
        assert_type(current, State)
        current.result = str(value.temperature)


assert_type(run, Action[Form])


def handler(value: Form, ctx: ActionContext[State]) -> None:
    pass


registered = app.action(handler, name="registered")
assert_type(registered, Action[Form])
checked = app.action(handler, name="checked", schema=schema.from_type(Form))
assert_type(checked, Action[Form])


@app.action
def no_input(value: None, ctx: ActionContext[State]) -> None:
    pass


def part(value: Local[float]) -> None:
    ui.slider("temperature", 0, 100, bind=value)


def main(state: State) -> None:
    form = ui.local("form", Form())
    assert_type(form, Local[Form])
    temperature = form.field("temperature", float)
    assert_type(temperature, Local[float])
    assert_type(temperature.value(), Ref[float])
    part(temperature)
    ui.slider("dynamic", 0, 100, bind=form.temperature)
    ui.button("Run", on_click=run, input=form)
    with ui.scope("partial"):
        ui.button("Run", on_click=run, input={"temperature": form.temperature})
    with ui.scope("explicit"):
        ui.button("Run", run.call(form))
        assert_type(run.compose({"temperature": temperature}), Ref[Form])
    ui.button("No input", on_click=no_input)
    initial: DictionaryForm = {"temperature": 50.0}
    dictionary = ui.local("dictionary", initial, type=DictionaryForm)
    assert_type(dictionary, Local[DictionaryForm])
    ui.text(state.result)


app.mount(main)


# Basic, untyped State does not require a framework model.
dictionary_app = App({"result": ""})
dictionary_app.mount(lambda state: ui.text(state["result"]))
