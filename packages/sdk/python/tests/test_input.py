import asyncio
import json
import unittest
from dataclasses import dataclass, field
from typing import NotRequired, TypedDict

from omochi_ui import App, DeclarationError, Expr, InputSchema, ProtocolError, schema


@dataclass
class Values:
    temperature: float


class ValuesExpr(TypedDict):
    temperature: float | Expr[float]


INPUT = InputSchema(Values, ValuesExpr)


class InputTests(unittest.IsolatedAsyncioTestCase):
    async def test_explicit_schemas_with_nested_custom_decoders(self):
        async def check_case(annotation, validator, raw, decoded):
            with self.subTest(annotation=annotation):
                calls = []
                app = App({})

                def run(value, ctx):
                    calls.append(value)

                run.__annotations__["value"] = annotation
                app.action(run, schema=validator)
                await app.build().invoke("run", raw)
                self.assertEqual(calls, [decoded])

        for annotation, validator, raw, decoded in [
            (list[int], schema.array(schema.custom(int)), ["1", "2"], [1, 2]),
            (
                dict[str, int],
                schema.map_(schema.custom(int)),
                {"count": "3"},
                {"count": 3},
            ),
            (
                tuple[str, int],
                schema.tuple_(schema.string(), schema.custom(int)),
                ["count", "4"],
                ("count", 4),
            ),
            (
                list[dict[str, tuple[int]]],
                schema.array(schema.map_(schema.tuple_(schema.custom(int)))),
                [{"count": ["5"]}],
                [{"count": (5,)}],
            ),
            (int, schema.custom(int), "6", 6),
            (int | None, schema.nullable(schema.custom(int)), "7", 7),
            (int | None, schema.nullable(schema.custom(int)), None, None),
            (
                tuple[int, int] | str,
                schema.union(
                    schema.string(), schema.tuple_(schema.integer(), schema.custom(int))
                ),
                [1, "8"],
                (1, 8),
            ),
            (
                tuple[int, int] | str,
                schema.union(
                    schema.tuple_(schema.integer(), schema.custom(int)), schema.string()
                ),
                "ready",
                "ready",
            ),
        ]:
            await check_case(annotation, validator, raw, decoded)

    def test_explicit_schemas_reject_known_nested_type_mismatches(self):
        for annotation, validator in [
            (list[int], schema.array(schema.string())),
            (dict[str, int], schema.map_(schema.string())),
            (tuple[str, int], schema.tuple_(schema.string(), schema.string())),
        ]:
            with self.subTest(annotation=annotation):
                app = App({})

                def run(value, ctx):
                    pass

                run.__annotations__["value"] = annotation
                with self.assertRaisesRegex(DeclarationError, "different types"):
                    app.action(run, schema=validator)

    def test_explicit_schemas_check_known_structure_beside_custom_decoders(self):
        for annotation, validator in [
            (tuple[int, int], schema.tuple_(schema.string(), schema.custom(int))),
            (tuple[int, int], schema.tuple_(schema.custom(int), schema.string())),
            (tuple[int, int], schema.tuple_(schema.custom(int))),
            (tuple[int], schema.tuple_(schema.custom(int), schema.integer())),
            (list[int], schema.map_(schema.custom(int))),
            (dict[str, int], schema.array(schema.custom(int))),
            (int, schema.array(schema.custom(int))),
            (
                list[tuple[int, int]],
                schema.array(schema.tuple_(schema.string(), schema.custom(int))),
            ),
            (
                dict[str, tuple[int, int]],
                schema.map_(schema.tuple_(schema.custom(int), schema.string())),
            ),
            (
                tuple[list[int], int],
                schema.tuple_(schema.map_(schema.custom(int)), schema.custom(int)),
            ),
            (
                tuple[int, int] | str,
                schema.union(
                    schema.string(), schema.tuple_(schema.string(), schema.custom(int))
                ),
            ),
            (int | None, schema.union(schema.string(), schema.custom(int))),
            (int, schema.nullable(schema.custom(int))),
        ]:
            with self.subTest(annotation=annotation, kind=validator.kind):
                app = App({})

                def run(value, ctx):
                    pass

                run.__annotations__["value"] = annotation
                with self.assertRaisesRegex(DeclarationError, "different types"):
                    app.action(run, schema=validator)

    async def test_dataclass_computed_fields_are_not_action_inputs(self):
        @dataclass
        class Computed:
            raw: str
            category: str = "general"
            normalized: str = field(init=False)
            cache: set[str] = field(init=False, default_factory=set)

            def __post_init__(self):
                self.normalized = self.raw.lower()

        class ComputedExpr(TypedDict):
            raw: str | Expr[str]
            category: NotRequired[str | Expr[str]]

        inputs = InputSchema(Computed, ComputedExpr)
        self.assertEqual(set(inputs.members), {"raw", "category"})
        inputs.field("raw", schema.string())
        with self.assertRaises(DeclarationError):
            inputs.field("normalized", schema.string())
        self.assertEqual(inputs.parse({"raw": "ABC"}), Computed("ABC"))
        with self.assertRaises(ProtocolError):
            inputs.parse({"raw": "ABC", "normalized": "forged"})
        calls = []
        app = App({})

        @app.action(schema=inputs)
        def run(value, ctx):
            calls.append(value)

        encoded = run.call(Computed("ABC")).input
        self.assertEqual(encoded, {"raw": "ABC", "category": "general"})
        runtime = app.build()
        await runtime.invoke("run", encoded)
        self.assertEqual(calls, [Computed("ABC")])
        self.assertEqual(calls[0].normalized, "abc")

        @dataclass
        class Envelope:
            item: Computed

        class EnvelopeExpr(TypedDict):
            item: ComputedExpr

        nested = InputSchema(Envelope, EnvelopeExpr)
        self.assertEqual(
            nested.parse({"item": {"raw": "ABC"}}), Envelope(Computed("ABC"))
        )
        from omochi_ui.expression import encode_input

        self.assertEqual(
            nested.parse(encode_input(Envelope(Computed("ABC")))),
            Envelope(Computed("ABC")),
        )

    def test_expression_results_can_be_members_or_subsets_of_unions(self):
        @dataclass
        class Nullable:
            value: str | None

        for annotation in [
            str | None | Expr[str],
            Expr[str],
            Expr[None],
            Expr[str | None],
            str | None | Expr[str | None],
        ]:
            with self.subTest(annotation=annotation):
                expression = TypedDict("NullableExpr", {"value": annotation})
                inputs = InputSchema(Nullable, expression)
                self.assertEqual(inputs.parse({"value": "x"}), Nullable("x"))
                self.assertEqual(inputs.parse({"value": None}), Nullable(None))
        for annotation in [
            Expr[int],
            Expr[str | int],
            str | None | Expr[int],
            Expr[Expr[str]],
        ]:
            with (
                self.subTest(annotation=annotation),
                self.assertRaises(DeclarationError),
            ):
                InputSchema(Nullable, TypedDict("InvalidExpr", {"value": annotation}))

        @dataclass
        class NonNullable:
            value: str

        with self.assertRaises(DeclarationError):
            InputSchema(
                NonNullable, TypedDict("TooBroadExpr", {"value": Expr[str | None]})
            )

    async def test_validation_before_handler_and_concrete_serialization(self):
        calls = []
        app = App({})

        @app.action(schema=INPUT)
        def run(value, ctx):
            calls.append(value)

        self.assertEqual(
            json.loads(json.dumps(run.call(Values(75)).input)), {"temperature": 75}
        )
        runtime = app.build()
        for value in [
            {},
            {"temperature": "bad"},
            {"temperature": True},
            {"temperature": float("nan")},
            {"temperature": 1, "extra": True},
        ]:
            with self.assertRaises(ProtocolError):
                await runtime.invoke("run", value)
        self.assertEqual(calls, [])
        await runtime.invoke("run", {"temperature": 75})
        self.assertEqual(calls, [Values(75)])

    async def test_blocks_release_lock_on_failure_and_reject_nesting(self):
        app = App({"n": 0})
        contexts = []

        @app.action(schema=schema.literal(None))
        async def run(_, ctx):
            contexts.append(ctx)
            with self.assertRaises(DeclarationError):
                async with ctx.update() as state:
                    state["n"] = 1
                    async with ctx.update():
                        pass
            self.assertEqual(app.state["n"], 1)
            async with ctx.update() as state:
                state["n"] = 2

        runtime = app.build()
        await asyncio.wait_for(runtime.invoke("run", None), 1)
        self.assertEqual(runtime.app_revision, 1)
        with self.assertRaises(DeclarationError):
            async with contexts[0].update():
                pass

    async def test_cancelled_block_keeps_mutation_without_revision(self):
        app = App({"n": 0})
        entered = asyncio.Event()

        @app.action(schema=schema.literal(None))
        async def run(_, ctx):
            async with ctx.update() as state:
                state["n"] = 1
                entered.set()
                await asyncio.Event().wait()

        @app.action(schema=schema.literal(None))
        async def next(_, ctx):
            async with ctx.update() as state:
                state["n"] += 1

        runtime = app.build()
        task = asyncio.create_task(runtime.invoke("run", None))
        await entered.wait()
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await task
        self.assertEqual(app.state["n"], 1)
        self.assertEqual(runtime.app_revision, 0)
        await asyncio.wait_for(runtime.invoke("next", None), 1)
        self.assertEqual(app.state["n"], 2)

    async def test_shared_validation_fixture(self):
        from pathlib import Path

        fixture = json.loads(
            (Path(__file__).parents[4] / "spec/fixtures/action-input.json").read_text()
        )
        validator = schema.object_(
            {"temperature": schema.number(), "note": schema.optional(schema.string())}
        )
        for value in fixture["valid"]:
            self.assertEqual(validator.parse(value), value)
        for value in fixture["invalid"]:
            with self.assertRaises(ProtocolError):
                validator.parse(value)

    async def test_shared_action_messages_preserve_input_precedence(self):
        from pathlib import Path

        fixture = json.loads(
            (Path(__file__).parents[4] / "spec/fixtures/action-input.json").read_text()
        )

        async def check_case(case):
            with self.subTest(name=case["name"]):
                calls = []
                app = App({})

                @app.action(schema=schema.nullable(schema.string()))
                def run(value: str | None, ctx):
                    calls.append(value)

                self.assertEqual(await app.build().handle(case["message"]), [])
                self.assertEqual(calls, [case["input"]])

        for case in fixture["messages"]:
            await check_case(case)

    async def test_complete_field_schema_identity(self):
        @dataclass
        class Maps:
            props: dict[str, str]

        obj = schema.from_type(Maps)
        obj.field("props", schema.map_(schema.string()))
        with self.assertRaises(DeclarationError):
            obj.field("props", schema.map_(schema.number()))
        for left, right in [
            (schema.tuple_(schema.string()), schema.tuple_(schema.number())),
            (
                schema.union(schema.string(), schema.integer()),
                schema.union(schema.string(), schema.boolean()),
            ),
            (schema.literal(1), schema.literal(True)),
            (
                schema.object_({"x": schema.string()}),
                schema.object_({"x": schema.number()}),
            ),
            (schema.array(schema.custom(str)), schema.array(schema.custom(str))),
        ]:
            owner = schema.object_({"value": left})
            with self.assertRaises(DeclarationError):
                owner.field("value", right)

    def test_optional_members_stay_optional_through_wrappers(self):
        for member, extra in [
            (schema.nullable(schema.optional(schema.string())), None),
            (schema.union(schema.number(), schema.optional(schema.string())), 42),
            (schema.union(schema.optional(schema.string()), schema.number()), 42),
            (
                schema.nullable(
                    schema.union(schema.number(), schema.optional(schema.string()))
                ),
                None,
            ),
        ]:
            with self.subTest(kind=member.kind):
                validator = schema.object_({"value": member})
                self.assertEqual(validator.parse({}), {})
                self.assertEqual(validator.parse({"value": "yes"}), {"value": "yes"})
                self.assertEqual(validator.parse({"value": extra}), {"value": extra})
                with self.assertRaises(ProtocolError):
                    validator.parse({"value": False})
        for member in [
            schema.nullable(schema.string()),
            schema.union(schema.number(), schema.string()),
        ]:
            with self.assertRaises(ProtocolError):
                schema.object_({"value": member}).parse({})

    async def test_whole_value_expression(self):
        app = App({})

        @app.action(schema=schema.string())
        def run(value: str, ctx):
            pass

        self.assertEqual(
            run.call(Expr[str]("/$local/name")).input, {"path": "/$local/name"}
        )

    async def test_merge_schema_and_fixture(self):
        from pathlib import Path

        from omochi_ui import (
            MERGE_OBJECTS_CAPABILITY,
            CapabilityError,
            Reconciler,
            merge_objects,
        )

        fixture = json.loads(
            (Path(__file__).parents[4] / "spec/fixtures/action-input.json").read_text()
        )
        title = schema.object_({"title": schema.string()})
        nested = schema.object_({"count": schema.integer()})
        result = schema.object_({"title": schema.string(), "count": schema.integer()})
        expr = merge_objects(
            result, title.input({"title": "x"}), nested.input(Expr("/$local/nested"))
        )
        app = App({})

        @app.action(schema=result)
        def run(value, ctx):
            pass

        self.assertEqual(run.call(expr).input, fixture["merge"]["expression"])

        @app.surface("main")
        def main(ui, state):
            ui.button("Run", run.call(expr))

        declaration = app.build().declaration("main")
        self.assertEqual(declaration.required_capabilities, {MERGE_OBJECTS_CAPABILITY})
        with self.assertRaises(CapabilityError):
            Reconciler().render(declaration)
        Reconciler({MERGE_OBJECTS_CAPABILITY}).render(declaration)
        with self.assertRaises(DeclarationError):
            merge_objects(result, title.input({"title": "x"}))
        with self.assertRaises(DeclarationError):
            merge_objects(
                result, nested.input({"count": 2}), nested.input({"count": 3})
            )
        with self.assertRaises(DeclarationError):
            merge_objects(
                nested,
                schema.object_({"count": schema.string()}).input({"count": "bad"}),
            )
        mapping = schema.map_(schema.integer())
        dynamic = merge_objects(
            mapping, mapping.input(Expr("/$local/map")), mapping.input({"count": 1})
        )
        from omochi_ui.expression import encode_input

        self.assertEqual(encode_input(dynamic)["call"], "omochiMergeObjects")
        with self.assertRaises(DeclarationError):
            run.call(dynamic)
