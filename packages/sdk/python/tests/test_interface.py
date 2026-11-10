from __future__ import annotations

import asyncio
import unittest
from dataclasses import dataclass, field
from typing import Literal, NotRequired, TypedDict

from omochi_ui import (
    UI,
    ActionContext,
    App,
    CallbackTransport,
    DeclarationError,
    Local,
    ProtocolError,
    schema,
    ui,
)


@dataclass
class Form:
    temperature: float = 50.0


@dataclass
class Nested:
    form: Form = field(default_factory=Form)
    values: list[float] = field(default_factory=list)


class MappingForm(TypedDict):
    temperature: float
    note: NotRequired[str]
    values: list[float]


@dataclass
class Restricted:
    status: Literal["ready"] = "ready"
    note: str | None = None


def nodes(declaration):
    result = {}

    def visit(node):
        result[node.key] = node
        for slot in node.slots:
            for child in slot.nodes:
                visit(child)

    visit(declaration.root)
    return result


class InterfaceTests(unittest.IsolatedAsyncioTestCase):
    async def test_inferred_actions_and_both_event_spellings_have_identical_wire_messages(
        self,
    ):
        def make(explicit):
            state = {"value": 0.0}
            app = App(state)

            @app.action
            async def run(value: Form, ctx: ActionContext[dict]):
                async with ctx.update() as current:
                    current["value"] = value.temperature

            def main(current):
                form = ui.local("form", Form())
                with ui.column():
                    ui.slider("temperature", 0, 100, bind=form.temperature)
                    if explicit:
                        ui.button("Run", run.call(form))
                    else:
                        ui.button("Run", on_click=run, input=form)
                    with ui.scope("partial"):
                        value = {"temperature": form.temperature}
                        if explicit:
                            ui.button("Partial", run.call(run.compose(value)))
                        else:
                            ui.button("Partial", on_click=run, input=value)
                    ui.text(str(current["value"]), key="result")

            app.mount(main)
            return app.build(), state

        runtime, state = make(False)
        explicit, _ = make(True)
        self.assertEqual(runtime.render(), explicit.render())
        await runtime.handle(
            {"action": {"name": "run", "context": {"input": {"temperature": 75}}}}
        )
        self.assertEqual(state["value"], 75.0)
        self.assertEqual(
            nodes(runtime.declaration("main"))["result"].props["text"], "75.0"
        )

    async def test_plain_state_is_neither_serialized_nor_copied(self):
        class ServerOnly:
            def __deepcopy__(self, memo):
                raise AssertionError("State must not be copied")

        state = {"resource": ServerOnly(), "count": 0}
        app = App(state)

        @app.action
        async def increment(value: None, ctx):
            async with ctx.update() as current:
                self.assertIs(current, state)
                current["count"] += 1

        def main(current):
            self.assertIs(current, state)
            ui.text(str(current["count"]))
            ui.button("Increment", on_click=increment)

        app.mount(main)
        runtime = app.build()
        runtime.render()
        await runtime.invoke("increment", None)
        self.assertEqual(state["count"], 1)

    async def test_typed_dict_preserves_absent_fields_and_empty_element_types(self):
        app = App({})

        @app.action
        def run(value: MappingForm, ctx):
            pass

        def main(state):
            form = ui.local(
                "form", {"temperature": 50.0, "values": []}, type=MappingForm
            )
            self.assertIsInstance(form.field("temperature", float), Local)
            form.field("values", list[float])
            with self.assertRaisesRegex(DeclarationError, "writable type"):
                form.field("values", list[str])
            form.field("note", str)
            with self.assertRaisesRegex(DeclarationError, "absent field"):
                ui.text_field("note", label="Note", bind=form.note)
            ui.button("Run", on_click=run, input=form)
            with self.assertRaisesRegex(DeclarationError, "type="):
                ui.local("untyped", {"temperature": 50.0})

        app.mount(main)
        declaration = app.build().declaration("main")
        self.assertEqual(
            declaration.locals["form"]["initial"], {"temperature": 50.0, "values": []}
        )

    async def test_writable_projection_cannot_widen_or_narrow_the_declaration(self):
        app = App({})

        def main(state):
            form = ui.local("form", Restricted())
            for name, kind in [("status", str), ("note", str)]:
                with self.assertRaisesRegex(DeclarationError, "writable type"):
                    form.field(name, kind)
            form.field("note", None | str)
            with self.assertRaisesRegex(DeclarationError, "writable type"):
                ui.text_field("status", label="Status", bind=form.status)
            flag = ui.local("flag", True)
            with self.assertRaisesRegex(DeclarationError, "writable type"):
                ui.slider("bad", 0, 1, bind=flag)
            with self.assertRaisesRegex(DeclarationError, "cannot project"):
                ui.local("number", 1.0)["real"]

        app.mount(main)
        app.build().declaration("main")

    async def test_nested_objects_arrays_maps_and_composition(self):
        app = App({})

        @app.action
        def run(value: Nested, ctx):
            pass

        def main(state):
            form = ui.local("form", Nested(values=[1.0, 2.0]))
            mapping = ui.local("map", {"x": 3.0}, type=dict[str, float])
            pair = ui.local("pair", (4.0, "x"), type=tuple[float, str])
            ui.slider("temperature", 0, 100, bind=form.form.temperature)
            value = {
                "form": {"temperature": mapping["x"]},
                "values": [form.values[0], pair[0]],
            }
            ui.button("Run", on_click=run, input=value)
            with self.assertRaisesRegex(DeclarationError, "known field"):
                run.compose({"form": {"typo": 1}})
            with self.assertRaises(DeclarationError):
                run.compose({"form": {"temperature": pair[1]}, "values": []})

        app.mount(main)
        call = nodes(app.build().declaration("main"))["action:run"].props["action"][
            "event"
        ]["context"]["input"]
        self.assertEqual(call["form"]["temperature"], {"path": "/$local/map/x"})
        self.assertEqual(
            call["values"],
            [{"path": "/$local/form/values/0"}, {"path": "/$local/pair/0"}],
        )

    async def test_repeated_components_have_stable_keys_and_drafts_when_reordered(self):
        app = App({"order": ["a/b", "a", 'x"]']})

        @app.action
        def run(value: Form, ctx):
            pass

        def part():
            form = ui.local("form", Form())
            with ui.column():
                ui.slider("temperature", 0, 100, bind=form.temperature)
                ui.button("Run", on_click=run, input=form)

        def main(state):
            for key in state["order"]:
                with ui.scope(key):
                    part()
                    if key == "a":
                        with ui.scope("b"):
                            part()

        app.mount(main)
        runtime = app.build()
        before = runtime.declaration("main")
        self.assertEqual(len(before.locals), 4)
        first = runtime.render()
        runtime.state["order"].reverse()
        after = runtime.declaration("main")
        self.assertEqual(before.locals, after.locals)
        self.assertEqual(set(nodes(before)), set(nodes(after)))
        updates = runtime.render()
        self.assertTrue(first)
        self.assertFalse(any("updateDataModel" in message for message in updates))

    async def test_namespace_and_layout_are_restored_after_exceptions(self):
        app = App({})

        def main(state):
            try:
                with ui.scope("failed"), ui.column():
                    raise ValueError("stop")
            except ValueError:
                pass
            ui.text("root", key="plain")
            ui.local("plain", "root")

        app.mount(main)
        declaration = app.build().declaration("main")
        self.assertIn("plain", nodes(declaration))
        self.assertIn("plain", declaration.locals)
        self.assertEqual(declaration.root.slots[0].nodes[-1].key, "plain")

    async def test_refs_and_cached_action_calls_cannot_escape_their_render(self):
        captured = []
        app = App({})

        @app.action
        def run(value: Form, ctx):
            pass

        def main(state):
            form = ui.local("form", Form())
            if captured:
                for stale in captured[0]:
                    with self.assertRaisesRegex(DeclarationError, "completed render"):
                        if hasattr(stale, "action"):
                            ui.button("Stale", stale)
                        else:
                            run.call(stale)
            captured.append(
                (form, run.compose({"temperature": form.temperature}), run.call(form))
            )

        app.mount(main)
        runtime = app.build()
        runtime.declaration("main")
        runtime.declaration("main")
        with self.assertRaisesRegex(DeclarationError, "completed render"):
            captured[0][0].value()

    async def test_different_surfaces_apps_and_nested_render_contexts(self):
        first, second = App({}), App({})

        @first.action
        def run(value: Form, ctx):
            pass

        second.mount(lambda state: ui.text("second"))
        other = second.build()
        first.mount(lambda state: ui.text("first"))
        cached = []

        def nested(state):
            form = ui.local("form", Form())
            cached.append(run.call(form))
            other.declaration("main")
            ui.button("Run", on_click=run, input=form)
            ui.text("restored", key="restored")

        first.mount(nested, surface_id="nested")
        declaration = first.build().declaration("nested")
        self.assertIn("restored", nodes(declaration))
        third = App({})

        def foreign(state):
            with self.assertRaisesRegex(DeclarationError, "different App"):
                ui.button("Run", cached[0])

        third.mount(foreign)
        third.build().declaration("main")

    async def test_facade_and_refs_reject_inherited_tasks_and_expired_contexts(self):
        tasks = []
        app = App({})

        async def child(form):
            with self.assertRaisesRegex(DeclarationError, "completed render"):
                ui.text("late")
            with self.assertRaisesRegex(DeclarationError, "completed render"):
                form.value()

        def main(state):
            form = ui.local("form", Form())
            tasks.append(asyncio.create_task(child(form)))

        app.mount(main)
        app.build().declaration("main")
        await asyncio.gather(*tasks)
        with self.assertRaisesRegex(DeclarationError, "while rendering"):
            ui.text("outside")
        with self.assertRaisesRegex(DeclarationError, "while rendering"), ui.column():
            pass

    async def test_render_failure_resets_facade_and_async_render_is_rejected(self):
        app = App({})

        def fail(state):
            raise ValueError("render failed")

        app.mount(fail)
        with self.assertRaisesRegex(ValueError, "render failed"):
            app.build().declaration("main")
        with self.assertRaises(DeclarationError):
            ui.text("outside")

        async def async_view(state):
            pass

        with self.assertRaisesRegex(DeclarationError, "synchronous"):
            App({}).mount(async_view)

    async def test_registration_and_event_errors_are_early_and_explicit(self):
        app = App({})

        def untyped(value, ctx):
            pass

        with self.assertRaisesRegex(DeclarationError, "annotation"):
            app.action(untyped)

        @app.action
        def run(value: Form, ctx):
            pass

        def main(state):
            form = ui.local("form", Form())
            for callback in [untyped, lambda: None]:
                with self.assertRaisesRegex(DeclarationError, "registered Action"):
                    ui.button("Run", on_click=callback, input=form)
            with self.assertRaisesRegex(DeclarationError, "requires input"):
                ui.button("Run", on_click=run)
            with self.assertRaisesRegex(DeclarationError, "combine"):
                ui.button("Run", run.call(form), on_click=run, input=form)
            with self.assertRaisesRegex(DeclarationError, "Python value"):
                bool(form.temperature)
            with ui.scope("same"):
                pass
            with (
                self.assertRaisesRegex(DeclarationError, "duplicate scope"),
                ui.scope("same"),
            ):
                pass
            with self.assertRaisesRegex(DeclarationError, "finite number"):
                ui.local("invalid", {"temperature": "wrong"}, type=Form)

        app.mount(main)
        app.build().declaration("main")

    async def test_literal_references_are_readable_as_strings_but_not_writable_as_strings(
        self,
    ):
        app = App({})

        @app.action
        def run(value: str, ctx):
            pass

        def main(state):
            form = ui.local("form", Restricted())
            ref = form.field("status", Literal["ready"])
            ui.button("Read", on_click=run, input=ref)
            with self.assertRaisesRegex(DeclarationError, "writable type"):
                ui.text_field("Write", label="Write", bind=ref)

        app.mount(main)
        app.build().declaration("main")

    async def test_composition_preserves_object_strip_policy(self):
        app = App({})

        @app.action(
            schema=schema.object_(
                {"temperature": schema.number()}, unknown_keys="strip"
            )
        )
        def run(value, ctx):
            pass

        def main(state):
            form = ui.local("form", Form())
            ui.button(
                "Run",
                on_click=run,
                input={"temperature": form.temperature, "ignored": "x"},
            )

        app.mount(main)
        value = nodes(app.build().declaration("main"))["action:run"].props["action"][
            "event"
        ]["context"]["input"]
        self.assertEqual(value, {"temperature": {"path": "/$local/form/temperature"}})

    async def test_annotation_errors_and_refinement_boundaries(self):
        app = App({})

        def wrong(value: int, ctx):
            pass

        with self.assertRaisesRegex(DeclarationError, "different types"):
            app.action(wrong, schema=schema.string())
        with self.assertRaisesRegex(DeclarationError, "input and context"):
            app.action(lambda: None)
        with self.assertRaisesRegex(DeclarationError, "state only"):
            app.mount(lambda ui, state: None)

    async def test_validation_errors_are_isolated_but_handler_errors_propagate(self):
        seen = []
        app = App({})

        @app.action(
            schema=schema.from_type(Form).refine(
                lambda value: value.temperature <= 100, "too hot"
            )
        )
        def run(value: Form, ctx):
            seen.append(value.temperature)

        async def messages():
            for value in ["invalid", {"temperature": 200}, {"temperature": 75}]:
                yield {"type": "invoke", "action": "run", "input": value}

        async def send(message):
            pass

        runtime = app.build()
        await runtime.start(CallbackTransport(send, messages))
        self.assertEqual(seen, [75.0])
        with self.assertRaises(ProtocolError):
            await runtime.invoke("run", {"temperature": 200})
        app = App({})

        @app.action("run")
        def fail(value: Form, ctx):
            raise ProtocolError("handler error")

        async def valid():
            yield {"type": "invoke", "action": "run", "input": {"temperature": 75}}

        with self.assertRaisesRegex(ProtocolError, "handler error"):
            await app.build().start(CallbackTransport(send, valid))

    async def test_custom_catalog_builder_is_available_in_mounted_views(self):
        from omochi_ui import Catalog

        class CustomUI(UI):
            def greeting(self):
                self.text("custom")

        catalog = Catalog("test", {}, ui_type=CustomUI)
        app = App({}, catalog=catalog)

        def main(state):
            self.assertIsInstance(ui.catalog(), CustomUI)
            ui.catalog(CustomUI).greeting()

        app.mount(main)
        self.assertEqual(
            next(
                node
                for node in nodes(app.build().declaration("main")).values()
                if node.component == "Text"
            ).props["text"],
            "custom",
        )

    async def test_composition_rejects_cycles_and_snapshots_literal_values(self):
        app = App({})

        @app.action
        def run(value: Nested, ctx):
            pass

        def main(state):
            form = ui.local("form", Form())
            value = {"form": form, "values": [1.0]}
            call = run.call(run.compose(value))
            value["values"].append(2.0)
            self.assertEqual(call.input["values"], [1.0])
            cycle = []
            cycle.append(cycle)
            with self.assertRaisesRegex(DeclarationError, "cyclic"):
                run.compose({"form": form, "values": cycle})

        app.mount(main)
        app.build().declaration("main")
