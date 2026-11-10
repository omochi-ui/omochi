from dataclasses import dataclass, field
from typing import TypedDict, assert_type
from omochi_ui import App, UI, ActionContext, Action, Expr, InputSchema, schema


@dataclass
class State:
    temperature: float = 50


@dataclass
class RunInput:
    temperature: float


class RunExpr(TypedDict):
    temperature: float | Expr[float]


inputs = InputSchema(RunInput, RunExpr)
temperature = inputs.field("temperature", schema.number())
app: App[State, UI] = App(State())


@app.action(schema=inputs)
async def run(value: RunInput, ctx: ActionContext[State]) -> None:
    async with ctx.update() as state:
        assert_type(state, State)
        state.temperature = value.temperature


assert_type(run, Action[RunInput, RunExpr])


@app.surface("main")
def view(ui: UI, state: State) -> None:
    form = ui.local("form", RunInput(state.temperature))
    ui.slider("temperature", 0, 100, bind=form.field(temperature).bind())
    ui.button("Run", run.call({"temperature": form.field(temperature).value()}))


from omochi_ui import merge_objects


@app.action(schema=schema.string())
def name_action(value: str, ctx: ActionContext[State]) -> None:
    pass


name_action.call(Expr[str]("/$local/name"))
run.call(
    merge_objects(
        schema.from_type(RunInput),
        schema.from_type(RunInput).input(Expr[RunInput]("/$local/form")),
    )
)


@dataclass
class Note:
    text: str | None
    normalized: str = field(init=False)

    def __post_init__(self) -> None:
        self.normalized = (self.text or "").lower()


class NoteExpr(TypedDict):
    text: str | None | Expr[str]


@app.action(schema=InputSchema(Note, NoteExpr))
def save_note(value: Note, ctx: ActionContext[State]) -> None:
    pass


save_note.call({"text": Expr[str]("/$local/name")})
save_note.call(Note("Hello"))
