from positive import app, run, State, RunInput, temperature
from omochi_ui import UI, ActionContext, Expr, schema

run.call({})  # error: missing temperature
run.call({"temperature": "wrong"})  # error: wrong value type

def view(ui: UI, state: State) -> None:
    local = ui.local("name", "text")
    ui.slider("temperature", 0, 100, bind=local.bind())  # error: wrong binding
    expr: Expr[float] = local.value()  # error: wrong expression
    local.field(temperature)  # error: wrong field owner

@app.action(schema=schema.string())
def handler(value: int, ctx: ActionContext[State]) -> None:  # error: wrong input
    pass
