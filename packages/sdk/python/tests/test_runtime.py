from __future__ import annotations

import asyncio
import json
import unittest
from dataclasses import dataclass
from typing import TypedDict
from pathlib import Path

from omochi_ui import schema, Expr, InputSchema, ActionContext, UI, App, CallbackTransport, DeclarationError, ProtocolError

ROOT = Path(__file__).parents[4]
FIXTURE = json.loads((ROOT / "spec/fixtures/basic.json").read_text())
CLIENT_ERRORS = json.loads((ROOT / "spec/fixtures/client-errors.json").read_text())


def messages_transport(messages, sent=None):
    async def send(message):
        if sent is not None: sent.append(message)
    async def receive():
        for message in messages: yield message
    return CallbackTransport(send, receive)

@dataclass
class RunInput:
    temperature: float

class RunInputExpr(TypedDict):
    temperature: float | Expr[float]

RUN_INPUT = InputSchema(RunInput, RunInputExpr)
TEMPERATURE = RUN_INPUT.field("temperature", schema.number())


def make_runtime():
    state = {"temperature": 50, "result": None}
    app = App(state)
    @app.action("run", schema=RUN_INPUT)
    async def run(input: RunInput, context):
        await context.update(lambda current: current.update(temperature=input.temperature, result=f"Result: {input.temperature:g}"))
    @app.surface("main")
    def main(ui, current):
        form = ui.local("form", RunInput(current["temperature"]))
        ui.text("Hello", key="hello")
        ui.slider("temperature", 0, 100, bind=form.field(TEMPERATURE).bind())
        ui.button("Run", run.call({"temperature": form.field(TEMPERATURE).value()}))
        if current["result"]: ui.text(current["result"], key="result")
    return app.build(), state

class RuntimeTests(unittest.TestCase):
    def test_client_reports_allow_subsequent_actions(self):
        async def scenario(report):
            runtime, state = make_runtime()
            self.assertEqual(await runtime.handle(report), [])
            sent = []
            await runtime.start(messages_transport([report, FIXTURE["action"]], sent))
            self.assertEqual(state["temperature"], 75)
            self.assertEqual(sent[1:], FIXTURE["initialMessages"] + FIXTURE["afterActionMessages"])
        for report in CLIENT_ERRORS["reports"]:
            with self.subTest(report=report): asyncio.run(scenario(report))

    def test_malformed_reports_and_unknown_messages_stop_receive_loop(self):
        async def scenario(message):
            runtime, state = make_runtime()
            with self.assertRaises(ProtocolError): await runtime.handle(message)
            with self.assertRaises(ProtocolError):
                await runtime.start(messages_transport([message, FIXTURE["action"]]))
            self.assertEqual(state["temperature"], 50)
        for message in CLIENT_ERRORS["invalid"]:
            with self.subTest(message=message): asyncio.run(scenario(message))

    def test_client_reports_do_not_mask_handler_failures(self):
        async def scenario(failure):
            app = App({})
            calls = []
            @app.action("fail", schema=schema.literal(None))
            def fail(_, context):
                calls.append(True)
                raise failure
            action = {"type": "invoke", "action": "fail"}
            with self.assertRaises(type(failure)) as caught:
                await app.build().start(messages_transport(CLIENT_ERRORS["reports"] + [action, action]))
            self.assertIs(caught.exception, failure)
            self.assertEqual(calls, [True])
        for failure in [RuntimeError("handler failed"), ProtocolError("handler failed")]:
            asyncio.run(scenario(failure))

    def test_shared_fixture(self):
        runtime, state = make_runtime()
        self.assertEqual(runtime.render(), FIXTURE["initialMessages"])
        self.assertEqual(asyncio.run(runtime.handle(FIXTURE["action"])), FIXTURE["afterActionMessages"])
        self.assertEqual(state["temperature"], 75)

    def test_action_is_independent_of_surface(self):
        state = {"value": 0}; app = App(state)
        @app.action("set", schema=schema.object_({"value": schema.integer()}))
        async def set_value(input, context): await context.update(lambda current: current.update(value=input["value"]))
        runtime = app.build()
        self.assertEqual(asyncio.run(runtime.invoke("set", {"value": 7})), [])
        self.assertEqual(state["value"], 7)

    def test_boolean_to_number_updates_action_input(self):
        app = App({"value": False})

        @app.action("set", schema=schema.union(schema.boolean(), schema.integer()))
        async def set_value(input, context):
            pass

        @app.surface("main")
        def main(ui, current):
            ui.button("Set", set_value.call(current["value"]))

        runtime = app.build()
        runtime.render()
        messages = asyncio.run(runtime.update(lambda current: current.update(value=0)))
        expected = [{"version": "v0.9", "updateComponents": {
            "surfaceId": "main", "components": [{
                "id": "action:set", "component": "Button",
                "action": {"event": {"name": "set", "context": {"input": 0}}},
                "child": "action:set:label",
            }],
        }}]
        self.assertEqual(json.dumps(messages, sort_keys=True), json.dumps(expected, sort_keys=True))

    def test_external_update_renders_all_surfaces(self):
        state = {"value": 0}; app = App(state)
        @app.surface("one")
        def one(ui, current): ui.text(str(current["value"]), key="value")
        @app.surface("two")
        def two(ui, current): ui.text(str(current["value"]), key="value")
        runtime = app.build(); runtime.render("one"); runtime.render("two")
        messages = asyncio.run(runtime.update(lambda current: current.update(value=1)))
        self.assertEqual({message["updateComponents"]["surfaceId"] for message in messages}, {"one", "two"})

    def test_concurrent_updates_are_coalesced(self):
        async def scenario():
            state = {"value": 0}; app = App(state)
            @app.surface("main")
            def main(ui, current): ui.text(str(current["value"]), key="value")
            runtime = app.build(); runtime.render()
            first, second = await asyncio.gather(runtime.update(lambda current: current.update(value=1)), runtime.update(lambda current: current.update(value=2)))
            return state, first, second
        state, first, second = asyncio.run(scenario())
        self.assertEqual(first, second)
        self.assertEqual(state["value"], 2)

    def test_local_initial_is_preserved_until_reset(self):
        state = {"value": "one", "revision": 0}; app = App(state)
        @app.surface("main")
        def main(ui, current):
            form = ui.local("form", {"draft": current["value"]}, reset_key=current["revision"])
            ui.text_field("draft", label="Draft", bind=form.field(schema.object_({"draft": schema.string()}).field("draft", schema.string())).bind())
        runtime = app.build(); runtime.render()
        self.assertEqual(asyncio.run(runtime.update(lambda current: current.update(value="two"))), [])
        self.assertEqual(asyncio.run(runtime.update(lambda current: current.update(revision=1))), [{"version": "v0.9", "updateDataModel": {"surfaceId": "main", "path": "/$local/form", "value": {"draft": "two"}}}])

    def test_get_surface_is_full_snapshot(self):
        runtime, _ = make_runtime(); runtime.render()
        self.assertIn("createSurface", asyncio.run(runtime.handle({"type": "getSurface", "surface": "main"}))[0])

    def test_duplicate_local_fails(self):
        app = App({})
        @app.surface("main")
        def main(ui, _): ui.local("x", {}); ui.local("x", {})
        with self.assertRaises(DeclarationError): app.build().render()

if __name__ == "__main__": unittest.main()
