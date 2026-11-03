from dataclasses import dataclass

from interface_positive import app, run, Form, State
from omochi_ui import ActionContext, Local, schema, ui


@dataclass
class OtherForm:
    temperature: str = "wrong"


def main(state: State) -> None:
    other = ui.local("other", OtherForm())
    run.call(other)  # error: different whole-value input
    ui.button("Run", on_click=run, input=other)  # error: no dynamic fallback for typed Local
    ui.slider("bad", 0, 100, bind=other.field("temperature", str))  # error: wrong writable type
    ui.slider("readonly", 0, 100, bind=ui.local("n", 1.0).value())  # error: read-only Ref
    ui.button("Missing", on_click=run)  # error: required input omitted
    ui.button("Raw callback", on_click=lambda: None, input=Form())  # error: not registered
    ui.button("Concrete", on_click=run, input="wrong")  # error: wrong concrete type
    run.call({"temperature": "wrong"})  # error: structured inputs require compose


def wrong_handler(value: int, ctx: ActionContext[State]) -> None:
    pass


app.action(wrong_handler, schema=schema.string())  # error: mismatched explicit schema


def part(value: Local[float]) -> None:
    pass


def invariance(value: Local[int]) -> None:
    part(value)  # error: cannot widen a writable Local
