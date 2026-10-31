import asyncio
import json
import unittest
from dataclasses import dataclass
from typing import TypedDict
from omochi_ui import App, Expr, InputSchema, schema, ProtocolError, DeclarationError

@dataclass
class Values:
    temperature: float

class ValuesExpr(TypedDict):
    temperature: float | Expr[float]

INPUT = InputSchema(Values, ValuesExpr)

class InputTests(unittest.IsolatedAsyncioTestCase):
    async def test_validation_before_handler_and_concrete_serialization(self):
        calls = []
        app = App({})
        @app.action(schema=INPUT)
        def run(value, ctx): calls.append(value)
        self.assertEqual(json.loads(json.dumps(run.call(Values(75)).input)), {"temperature":75})
        runtime = app.build()
        for value in [{}, {"temperature":"bad"}, {"temperature":True}, {"temperature":float("nan")}, {"temperature":1,"extra":True}]:
            with self.assertRaises(ProtocolError): await runtime.invoke("run", value)
        self.assertEqual(calls, [])
        await runtime.invoke("run", {"temperature":75})
        self.assertEqual(calls, [Values(75)])

    async def test_blocks_release_lock_on_failure_and_reject_nesting(self):
        app = App({"n":0})
        contexts = []
        @app.action(schema=schema.literal(None))
        async def run(_, ctx):
            contexts.append(ctx)
            with self.assertRaises(DeclarationError):
                async with ctx.update() as state:
                    state["n"] = 1
                    async with ctx.update(): pass
            self.assertEqual(app.state["n"], 1)
            async with ctx.update() as state: state["n"] = 2
        runtime = app.build()
        await asyncio.wait_for(runtime.invoke("run", None), 1)
        self.assertEqual(runtime.app_revision, 1)
        with self.assertRaises(DeclarationError):
            async with contexts[0].update(): pass

    async def test_cancelled_block_keeps_mutation_without_revision(self):
        app = App({"n":0})
        entered = asyncio.Event()
        @app.action(schema=schema.literal(None))
        async def run(_, ctx):
            async with ctx.update() as state:
                state["n"] = 1
                entered.set()
                await asyncio.Event().wait()
        @app.action(schema=schema.literal(None))
        async def next(_, ctx):
            async with ctx.update() as state: state["n"] += 1
        runtime = app.build()
        task = asyncio.create_task(runtime.invoke("run", None))
        await entered.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError): await task
        self.assertEqual(app.state["n"], 1)
        self.assertEqual(runtime.app_revision, 0)
        await asyncio.wait_for(runtime.invoke("next", None), 1)
        self.assertEqual(app.state["n"], 2)

    async def test_shared_validation_fixture(self):
        from pathlib import Path
        fixture = json.loads((Path(__file__).parents[4] / "spec/fixtures/action-input.json").read_text())
        validator = schema.object_({"temperature": schema.number(), "note": schema.optional(schema.string())})
        for value in fixture["valid"]: self.assertEqual(validator.parse(value), value)
        for value in fixture["invalid"]:
            with self.assertRaises(ProtocolError): validator.parse(value)
