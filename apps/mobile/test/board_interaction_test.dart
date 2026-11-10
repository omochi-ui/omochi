import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/remote/board_canvas.dart';
import 'package:omochi_mobile/src/remote/board_models.dart';
import 'package:sliver_dashboard/sliver_dashboard.dart' as grid;

import 'support/renderer_fixture.dart';

BoardPanel panel(String id, int y) => BoardPanel(
  id: id,
  title: id,
  reference: SurfaceReference.fromSurfaceId(formId),
  x: 0,
  y: y,
  w: 12,
  h: 8,
);

void main() {
  for (final cancellation in ['width', 'parent', 'lifecycle', 'escape']) {
    testWidgets('resize cancellation restores draft after $cancellation', (
      tester,
    ) async {
      tester.view.physicalSize = const Size(1100, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final message in demoMessages()) {
        hub.receive(message);
      }
      final changes = <List<BoardPanel>>[];
      Widget app() => MaterialApp(
        theme: ThemeData(platform: TargetPlatform.linux),
        home: Scaffold(
          body: BoardCanvas(
            panels: [panel('Form', 0)],
            session: hub,
            interact: true,
            onChange: changes.add,
          ),
        ),
      );
      await tester.pumpWidget(app());
      await tester.pumpAndSettle();
      final input = find.byType(TextField);
      await tester.enterText(input, 'Retained after cancellation');
      final state = tester.state(input);
      final controller = tester
          .widget<grid.DashboardOverlay>(find.byType(grid.DashboardOverlay))
          .controller;
      final original = controller.layout.value.single.toMap();
      Future<TestGesture> resize() => tester.startGesture(
        tester.getRect(find.byType(Card)).bottomRight - const Offset(4, 4),
        kind: ui.PointerDeviceKind.mouse,
      );
      final gesture = await resize();
      await gesture.moveBy(const Offset(0, 56));
      await tester.pump();
      expect((controller as dynamic).isResizing.value, true);
      switch (cancellation) {
        case 'width':
          tester.view.physicalSize = const Size(1150, 900);
        case 'parent':
          await tester.pumpWidget(app());
        case 'lifecycle':
          tester.binding.handleAppLifecycleStateChanged(
            AppLifecycleState.inactive,
          );
        case 'escape':
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      }
      await tester.pump();
      expect((controller as dynamic).isResizing.value, false);
      await gesture.moveBy(const Offset(0, 56));
      await tester.pump();
      expect(tester.takeException(), isNull);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(controller.layout.value.single.toMap(), original);
      expect(changes, isEmpty);
      expect(tester.state(input), same(state));
      expect(
        tester.widget<TextField>(input).controller!.text,
        'Retained after cancellation',
      );
      if (cancellation == 'lifecycle') {
        tester.binding.handleAppLifecycleStateChanged(
          AppLifecycleState.resumed,
        );
      }
      final next = await resize();
      await next.moveBy(const Offset(0, 56));
      await tester.pump();
      await next.up();
      await tester.pumpAndSettle();
      expect(changes, hasLength(1));
      expect(changes.single.single.h, greaterThan(8));
      expect(tester.state(input), same(state));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    }, variant: TargetPlatformVariant({TargetPlatform.linux}));
  }

  for (final cancellation in ['escape', 'width']) {
    testWidgets('narrow reorder cancels after $cancellation', (tester) async {
      tester.view.physicalSize = const Size(600, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final hub = HubSession();
      addTearDown(hub.dispose);
      final changes = <List<BoardPanel>>[];
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(platform: TargetPlatform.linux),
          home: Scaffold(
            body: BoardCanvas(
              panels: [panel('a', 0), panel('b', 8)],
              session: hub,
              interact: false,
              onChange: changes.add,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final gesture = await tester.startGesture(
        tester.getCenter(find.byIcon(Icons.drag_handle).first),
        kind: ui.PointerDeviceKind.mouse,
      );
      await gesture.moveBy(const Offset(0, 448));
      await tester.pump(const Duration(milliseconds: 500));
      if (cancellation == 'escape') {
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      } else {
        tester.view.physicalSize = const Size(1100, 900);
      }
      await tester.pumpAndSettle();
      await gesture.moveBy(const Offset(0, 20));
      await gesture.up();
      await tester.pumpAndSettle();
      expect(changes, isEmpty);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    }, variant: TargetPlatformVariant({TargetPlatform.linux}));
  }

  for (final width in [600.0, 1100.0]) {
    testWidgets('retained app input reveals after keyboard at $width', (
      tester,
    ) async {
      tester.view.physicalSize = Size(width, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetViewInsets);
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final message in demoMessages()) {
        hub.receive(message);
      }
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: BoardCanvas(
              panels: [for (var i = 0; i < 5; i++) panel('Form$i', i * 8)],
              session: hub,
              interact: true,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final state = tester.state(find.byType(TextField).at(1));
      final input = find.byElementPredicate(
        (element) => identical(element, state.context),
        skipOffstage: false,
      );
      await tester.enterText(input, 'Reveal retained input');
      await tester.pumpAndSettle();
      tester.view.viewInsets = const FakeViewPadding(bottom: 400);
      await tester.pumpAndSettle();
      expect(tester.getRect(input).bottom, lessThanOrEqualTo(500));
      expect(tester.getRect(input).top, greaterThanOrEqualTo(0));
      final scrollable = tester.state<ScrollableState>(
        find.byType(Scrollable).first,
      );
      scrollable.position.jumpTo(0);
      await tester.pumpAndSettle();
      await Scrollable.ensureVisible(state.context);
      await tester.pumpAndSettle();
      expect(tester.getRect(input).bottom, lessThanOrEqualTo(500));
      expect(tester.getRect(input).top, greaterThanOrEqualTo(0));
      scrollable.position.jumpTo(scrollable.position.maxScrollExtent);
      FocusManager.instance.primaryFocus?.unfocus();
      await tester.pumpAndSettle();
      await Scrollable.ensureVisible(state.context);
      await tester.pumpAndSettle();
      expect(tester.getRect(input).bottom, lessThanOrEqualTo(500));
      expect(tester.getRect(input).top, greaterThanOrEqualTo(0));
      expect(input.hitTestable(), findsOneWidget);
      expect(tester.state(input), same(state));
      expect(
        tester.widget<TextField>(input).controller!.text,
        'Reveal retained input',
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    }, variant: TargetPlatformVariant({TargetPlatform.android}));
  }
}
