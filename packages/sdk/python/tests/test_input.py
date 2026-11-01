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

    async def test_complete_field_schema_identity(self):
        @dataclass
        class Maps:
            props: dict[str, str]
        obj = schema.from_type(Maps)
        obj.field("props", schema.map_(schema.string()))
        with self.assertRaises(DeclarationError): obj.field("props", schema.map_(schema.number()))
        for left, right in [
            (schema.tuple_(schema.string()), schema.tuple_(schema.number())),
            (schema.union(schema.string(), schema.integer()), schema.union(schema.string(), schema.boolean())),
            (schema.literal(1), schema.literal(True)),
            (schema.object_({"x":schema.string()}), schema.object_({"x":schema.number()})),
            (schema.array(schema.custom(str)), schema.array(schema.custom(str))),
        ]:
            owner = schema.object_({"value": left})
            with self.assertRaises(DeclarationError): owner.field("value", right)

    def test_optional_members_stay_optional_through_wrappers(self):
        for member, extra in [
            (schema.nullable(schema.optional(schema.string())), None),
            (schema.union(schema.number(), schema.optional(schema.string())), 42),
            (schema.union(schema.optional(schema.string()), schema.number()), 42),
            (schema.nullable(schema.union(schema.number(), schema.optional(schema.string()))), None),
        ]:
            with self.subTest(kind=member.kind):
                validator = schema.object_({"value": member})
                self.assertEqual(validator.parse({}), {})
                self.assertEqual(validator.parse({"value": "yes"}), {"value": "yes"})
                self.assertEqual(validator.parse({"value": extra}), {"value": extra})
                with self.assertRaises(ProtocolError): validator.parse({"value": False})
        for member in [schema.nullable(schema.string()), schema.union(schema.number(), schema.string())]:
            with self.assertRaises(ProtocolError): schema.object_({"value": member}).parse({})

    async def test_whole_value_expression(self):
        app = App({})
        @app.action(schema=schema.string())
        def run(value: str, ctx): pass
        self.assertEqual(run.call(Expr[str]("/$local/name")).input, {"path":"/$local/name"})

    async def test_merge_schema_and_fixture(self):
        from pathlib import Path
        from omochi_ui import merge_objects, MERGE_OBJECTS_CAPABILITY, CapabilityError, Reconciler
        fixture = json.loads((Path(__file__).parents[4] / "spec/fixtures/action-input.json").read_text())
        title = schema.object_({"title": schema.string()})
        nested = schema.object_({"count": schema.integer()})
        result = schema.object_({"title": schema.string(), "count": schema.integer()})
        expr = merge_objects(result, title.input({"title":"x"}), nested.input(Expr("/$local/nested")))
        app = App({})
        @app.action(schema=result)
        def run(value, ctx): pass
        self.assertEqual(run.call(expr).input, fixture["merge"]["expression"])
        @app.surface("main")
        def main(ui, state): ui.button("Run", run.call(expr))
        declaration = app.build().declaration("main")
        self.assertEqual(declaration.required_capabilities, {MERGE_OBJECTS_CAPABILITY})
        with self.assertRaises(CapabilityError): Reconciler().render(declaration)
        Reconciler({MERGE_OBJECTS_CAPABILITY}).render(declaration)
        with self.assertRaises(DeclarationError): merge_objects(result, title.input({"title":"x"}))
        with self.assertRaises(DeclarationError): merge_objects(result, nested.input({"count":2}), nested.input({"count":3}))
        with self.assertRaises(DeclarationError): merge_objects(nested, schema.object_({"count":schema.string()}).input({"count":"bad"}))
        mapping = schema.map_(schema.integer())
        dynamic = merge_objects(mapping, mapping.input(Expr("/$local/map")), mapping.input({"count":1}))
        self.assertEqual(run.call(dynamic).input["call"], "omochiMergeObjects")
