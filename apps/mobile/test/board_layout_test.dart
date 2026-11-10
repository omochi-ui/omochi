import 'package:flutter/services.dart';
import 'package:sliver_dashboard/sliver_dashboard.dart' as grid;

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';
import 'package:genui/genui.dart';
import 'package:omochi_mobile/src/remote/board_layout.dart';
import 'package:omochi_mobile/src/remote/board_models.dart';
import 'package:omochi_mobile/src/remote/board_canvas.dart';
import 'package:omochi_mobile/src/remote/board_surface.dart';
import 'package:omochi_mobile/src/hub_session.dart';

import 'support/renderer_fixture.dart';

BoardPanel panel(String id, int x, int y, int w, int h) => BoardPanel(
  id: id,
  title: id,
  reference: SurfaceReference.fromSurfaceId(formId),
  x: x,
  y: y,
  w: w,
  h: h,
);
void main() {
  test(
    'packing preserves requested order and sizes, including uneven rows',
    () {
      final result = packPanels([
        panel('a', 0, 99, 8, 2),
        panel('b', 0, 0, 6, 7),
        panel('c', 0, 0, 6, 2),
      ]);
      expect(validLayout(result), true);
      expect(readingOrder(result).map((p) => p.id), ['a', 'b', 'c']);
      expect(result.map((p) => p.y), [0, 2, 2]);
      expect(validLayout([panel('a', 11, 0, 2, 1)]), false);
      expect(
        validLayout([panel('a', 0, 0, 2, 2), panel('b', 1, 1, 2, 2)]),
        false,
      );
      expect(validLayout([panel('a', 0, 100001, 1, 1)]), false);
    },
  );
  for (final (name, viewport, keyboardInset) in [
    ('wide tall panel', const Size(1100, 500), 0.0),
    ('narrow landscape panel', const Size(600, 300), 0.0),
    ('keyboard-reduced narrow panel', const Size(600, 700), 400.0),
  ]) {
    testWidgets('$name keeps full GenUI body size and input hit area', (
      tester,
    ) async {
      tester.view.physicalSize = viewport;
      tester.view.devicePixelRatio = 1;
      tester.view.viewInsets = FakeViewPadding(bottom: keyboardInset);
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      addTearDown(tester.view.resetViewInsets);
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final message in surfaceMessages(
        formId,
        [
          {
            'id': 'root',
            'component': 'Column',
            'justify': 'end',
            'children': ['input'],
          },
          {
            'id': 'input',
            'component': 'TextField',
            'label': 'Bottom input',
            'value': {'path': '/value'},
          },
        ],
        {'value': ''},
      )) {
        hub.receive(message);
      }
      var writes = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: BoardCanvas(
              panels: [
                panel('First', 0, 0, 12, 20),
                panel('Second', 0, 20, 12, 20),
              ],
              session: hub,
              interact: true,
              onChange: (_) => writes++,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final slot = find.byType(BoardSurfaceSlot).first;
      final anchor = tester.widget<BoardSurfaceSlot>(slot).anchor;
      final slotSize = tester.getSize(slot);
      final canvasRect = tester.getRect(find.byType(BoardCanvas));
      expect(
        slotSize,
        Size(viewport.width - 32, viewport.width > 760 ? 1056 : 552),
      );
      expect(slotSize.height, greaterThan(canvasRect.height));
      expect(tester.getSize(find.byType(Surface).first), slotSize);
      final state = tester.state(find.byType(TextField).first);
      final input = find.byElementPredicate(
        (element) => identical(element, state.context),
        skipOffstage: false,
      );
      final focus = tester
          .widget<EditableText>(
            find.descendant(of: input, matching: find.byType(EditableText)),
          )
          .focusNode;
      // Exercise the part of the follower beyond its original viewport height.
      expect(
        tester.getRect(input).top - tester.getRect(slot).top,
        greaterThan(canvasRect.height),
      );
      expect(input.hitTestable(), findsNothing);
      final scrollable = tester.state<ScrollableState>(
        find
            .descendant(
              of: find.byType(BoardScrollView),
              matching: find.byType(Scrollable),
            )
            .first,
      );
      final inputOffset = tester.getRect(input).bottom - canvasRect.bottom + 16;
      Future<void> revealAndFocus() async {
        scrollable.position.jumpTo(inputOffset);
        await tester.pumpAndSettle();
        expect(input.hitTestable(), findsOneWidget);
        await tester.tap(input);
        await tester.pumpAndSettle();
        expect(focus.hasFocus, true);
      }

      await revealAndFocus();
      await tester.enterText(input, 'Retained bottom input');
      focus.unfocus();
      await tester.pumpAndSettle();
      scrollable.position.jumpTo(scrollable.position.maxScrollExtent);
      await tester.pumpAndSettle();
      expect(anchor.link.leader, isNull);
      expect(state.mounted, true);
      expect(input.hitTestable(), findsNothing);
      await revealAndFocus();
      expect(tester.state(input), same(state));
      expect(
        tester.widget<TextField>(input).controller!.text,
        'Retained bottom input',
      );
      expect(tester.getSize(find.byType(Surface).first), slotSize);
      expect(writes, 0);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    });
  }
  for (final startWidth in [600.0, 1100.0]) {
    testWidgets(
      'offscreen bottom input reveal follows breakpoint from $startWidth',
      (tester) async {
        tester.view.physicalSize = Size(startWidth, 500);
        tester.view.devicePixelRatio = 1;
        addTearDown(tester.view.resetPhysicalSize);
        addTearDown(tester.view.resetDevicePixelRatio);
        final hub = HubSession();
        addTearDown(hub.dispose);
        for (final message in surfaceMessages(
          formId,
          [
            {
              'id': 'root',
              'component': 'Column',
              'justify': 'end',
              'children': ['input'],
            },
            {
              'id': 'input',
              'component': 'TextField',
              'label': 'Bottom input',
              'value': {'path': '/value'},
            },
          ],
          {'value': ''},
        )) {
          hub.receive(message);
        }
        final panels = [
          for (var i = 0; i < 8; i++) panel('Form$i', 0, i * 20, 12, 20),
        ];
        var writes = 0;
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: BoardCanvas(
                panels: panels,
                session: hub,
                interact: true,
                onChange: (_) => writes++,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        final hosts = tester
            .widgetList<BoardSurfaceHost>(find.byType(BoardSurfaceHost))
            .toList();
        expect(hosts.last.anchor.size.value, isNull);
        final scrollable = tester.state<ScrollableState>(
          find
              .descendant(
                of: find.byType(BoardScrollView),
                matching: find.byType(Scrollable),
              )
              .first,
        );
        // On the wide-to-narrow path retain a later panel and return to the top,
        // so the narrow list never needs to build that slot on resize.
        final targetIndex = startWidth == 1100 ? 4 : 0;
        final anchor = hosts[targetIndex].anchor;
        if (targetIndex > 0) {
          scrollable.position.jumpTo(4480);
          await tester.pumpAndSettle();
        }
        final host = find.byWidgetPredicate(
          (widget) =>
              widget is BoardSurfaceHost && identical(widget.anchor, anchor),
        );
        final state = tester.state(
          find.descendant(of: host, matching: find.byType(TextField)).first,
        );
        final input = find.byElementPredicate(
          (element) => identical(element, state.context),
          skipOffstage: false,
        );
        final focus = tester
            .widget<EditableText>(
              find.descendant(of: input, matching: find.byType(EditableText)),
            )
            .focusNode;
        await Scrollable.ensureVisible(state.context);
        await tester.pumpAndSettle();
        await tester.enterText(input, 'Retained bottom input');
        focus.unfocus();
        await tester.pumpAndSettle();
        scrollable.position.jumpTo(
          startWidth == 1100 ? 0 : scrollable.position.maxScrollExtent,
        );
        await tester.pumpAndSettle();
        expect(anchor.link.leader, isNull);

        tester.view.physicalSize = Size(startWidth == 600 ? 1100 : 600, 500);
        await tester.pumpAndSettle();
        expect(anchor.link.leader, isNull);
        expect(input.hitTestable(), findsNothing);

        // One reveal must use the destination layout even before its sliver
        // slot is rebuilt. Focus must not be needed to correct a stale reveal.
        await Scrollable.ensureVisible(state.context);
        await tester.pumpAndSettle();
        final canvas = tester.getRect(find.byType(BoardCanvas));
        final inputRect = tester.getRect(input);
        expect(inputRect.top, greaterThanOrEqualTo(canvas.top));
        expect(inputRect.bottom, lessThanOrEqualTo(canvas.bottom));
        expect(input.hitTestable(), findsOneWidget);
        final slot = find.byWidgetPredicate(
          (widget) =>
              widget is BoardSurfaceSlot && identical(widget.anchor, anchor),
        );
        final surface = find.ancestor(
          of: input,
          matching: find.byType(Surface),
        );
        expect(tester.getSize(surface), tester.getSize(slot));
        await tester.tap(input);
        await tester.pumpAndSettle();
        expect(focus.hasFocus, true);
        expect(tester.state(input), same(state));
        expect(
          tester.widget<TextField>(input).controller!.text,
          'Retained bottom input',
        );
        expect(writes, 0);
        expect(tester.takeException(), isNull);
        await tester.pumpWidget(const SizedBox());
      },
    );
  }
  testWidgets('breakpoint projection keeps unvisited GenUI lazy', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final hub = HubSession();
    addTearDown(hub.dispose);
    for (final message in demoMessages()) {
      hub.receive(message);
    }
    tester.view.physicalSize = const Size(600, 500);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: BoardCanvas(
            panels: [
              for (var i = 0; i < 8; i++) panel('Form$i', 0, i * 20, 12, 20),
            ],
            session: hub,
            interact: true,
          ),
        ),
      ),
    );
    for (final width in [600.0, 1100.0, 600.0]) {
      tester.view.physicalSize = Size(width, 500);
      await tester.pumpAndSettle();
      expect(
        find.byType(TextField, skipOffstage: false).evaluate().length,
        lessThan(8),
      );
      final hosts = tester.widgetList<BoardSurfaceHost>(
        find.byType(BoardSurfaceHost),
      );
      expect(hosts.last.anchor.size.value, isNull);
    }
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });
  testWidgets(
    'width switching retains real GenUI input state without layout writes',
    (tester) async {
      if (const bool.fromEnvironment('BOARD_SCREENSHOTS')) {
        await tester.runAsync(() async {
          const root = String.fromEnvironment('BOARD_FONT_DIR');
          for (final entry in {
            'BoardPreview': 'Roboto-Regular.ttf',
            'MaterialIcons': 'MaterialIcons-Regular.otf',
          }.entries) {
            final loader = FontLoader(entry.key)
              ..addFont(
                Future.value(
                  ByteData.sublistView(
                    await File('$root/${entry.value}').readAsBytes(),
                  ),
                ),
              );
            await loader.load();
          }
        });
      }
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final msg in demoMessages()) {
        hub.receive(msg);
      }
      var writes = 0;
      final previewKey = GlobalKey();
      Future<void> show(double width) async {
        tester.view.resetPhysicalSize();
        tester.view.physicalSize = Size(width, 900);
        tester.view.devicePixelRatio = 1;
        await tester.pumpWidget(
          RepaintBoundary(
            key: previewKey,
            child: MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: ThemeData(
                fontFamily: const bool.fromEnvironment('BOARD_SCREENSHOTS')
                    ? 'BoardPreview'
                    : null,
              ),
              home: Scaffold(
                body: BoardCanvas(
                  panels: [
                    panel('Form', 0, 0, 12, 8),
                    for (var i = 1; i < 5; i++)
                      BoardPanel(
                        id: 'offline$i',
                        title: 'Offline $i',
                        reference: const SurfaceReference(
                          'offline',
                          'app',
                          'main',
                        ),
                        x: 0,
                        y: i * 8,
                        w: 12,
                        h: 8,
                      ),
                  ],
                  session: hub,
                  interact: true,
                  onChange: (_) => writes++,
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
        if (const bool.fromEnvironment('BOARD_SCREENSHOTS')) {
          await tester.runAsync(() async {
            final boundary = tester.renderObject<RenderRepaintBoundary>(
              find.byKey(previewKey),
            );
            final image = await boundary.toImage();
            final bytes = await image.toByteData(
              format: ui.ImageByteFormat.png,
            );
            await File('/tmp/omochi-board-${width.toInt()}.png')
                .writeAsBytes(bytes!.buffer.asUint8List());
            image.dispose();
          });
        }
      }

      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      await show(1100);
      final input = find.byType(TextField).first;
      await tester.enterText(input, 'Retained');
      final state = tester.state(input);
      await show(600);
      expect(tester.state(find.byType(TextField).first), same(state));
      expect(find.text('Retained'), findsOneWidget);
      await tester.drag(
        find.byWidgetPredicate((widget) => widget is CustomScrollView),
        const Offset(0, -1800),
      );
      await tester.pumpAndSettle();
      await tester.drag(
        find.byWidgetPredicate((widget) => widget is CustomScrollView),
        const Offset(0, 2500),
      );
      await tester.pumpAndSettle();
      expect(tester.state(find.byType(TextField).first), same(state));
      await show(1100);
      expect(tester.state(find.byType(TextField).first), same(state));
      expect(writes, 0);
      await tester.pumpWidget(const SizedBox());
    },
  );
  testWidgets('desktop app input leaves saved gaps untouched', (tester) async {
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
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(platform: TargetPlatform.linux),
        home: Scaffold(
          body: BoardCanvas(
            panels: [panel('Form', 0, 2, 12, 8)],
            session: hub,
            interact: true,
            onChange: changes.add,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final controller = tester
        .widget<grid.DashboardOverlay>(find.byType(grid.DashboardOverlay))
        .controller;
    await tester.enterText(find.byType(TextField), 'Retained through editing');
    final state = tester.state(find.byType(TextField));
    await tester.tap(find.byType(TextField), kind: ui.PointerDeviceKind.mouse);
    await tester.pumpAndSettle();
    expect(controller.layout.value.single.y, 2);
    expect(changes, isEmpty);
    await tester.drag(
      find.byType(Slider),
      const Offset(100, 0),
      kind: ui.PointerDeviceKind.mouse,
    );
    await tester.pumpAndSettle();
    expect(controller.layout.value.single.y, 2);
    expect(changes, isEmpty);
    await tester.drag(
      find.byIcon(Icons.drag_handle),
      const Offset(0, 112),
      kind: ui.PointerDeviceKind.mouse,
    );
    await tester.pumpAndSettle();
    expect(changes, isNotEmpty);
    expect(tester.state(find.byType(TextField)), same(state));
    final beforeResize = controller.layout.value.single;
    final rect = tester.getRect(find.byType(Card));
    final resize = await tester.startGesture(
      rect.bottomRight - const Offset(4, 4),
      kind: ui.PointerDeviceKind.mouse,
    );
    await tester.pump();
    await resize.moveBy(const Offset(0, 56));
    await tester.pump();
    await resize.up();
    await tester.pumpAndSettle();
    expect(controller.layout.value.single.h, greaterThan(beforeResize.h));
    expect(tester.state(find.byType(TextField)), same(state));
    expect(
      (state.widget as TextField).controller!.text,
      'Retained through editing',
    );
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  }, variant: TargetPlatformVariant({TargetPlatform.linux}));

  testWidgets(
    'wide mobile edge input works during layout editing',
    (tester) async {
      tester.view.physicalSize = const Size(1100, 900);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final message in surfaceMessages(
        formId,
        [
          {
            'id': 'root',
            'component': 'TextField',
            'label': 'Input',
            'value': {'path': '/value'},
          },
        ],
        {'value': ''},
      )) {
        hub.receive(message);
      }
      final changes = <List<BoardPanel>>[];
      await tester.pumpWidget(
        MaterialApp(
          // Resize support follows the runtime platform, even with another theme.
          theme: ThemeData(platform: TargetPlatform.linux),
          home: Scaffold(
            body: BoardCanvas(
              panels: [panel('Form', 0, 2, 12, 8)],
              session: hub,
              interact: true,
              onChange: changes.add,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final input = find.byType(TextField);
      final state = tester.state(input);
      final focus = tester
          .widget<EditableText>(find.byType(EditableText))
          .focusNode;
      final rect = tester.getRect(input);
      for (final point in [
        Offset(rect.left + 10, rect.center.dy),
        Offset(rect.right - 10, rect.center.dy),
        Offset(rect.center.dx, rect.bottom - 10),
      ]) {
        await tester.tapAt(point);
        await tester.pumpAndSettle();
        expect(focus.hasFocus, true, reason: 'App input at $point must focus');
        focus.unfocus();
        await tester.pumpAndSettle();
      }
      final controller = tester
          .widget<grid.DashboardOverlay>(find.byType(grid.DashboardOverlay))
          .controller;
      expect(controller.layout.value.single.y, 2);
      expect(changes, isEmpty);
      await tester.drag(find.byIcon(Icons.drag_handle), const Offset(0, 112));
      await tester.pumpAndSettle();
      expect(changes, isNotEmpty);
      expect(tester.state(input), same(state));
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
    variant: TargetPlatformVariant({
      TargetPlatform.android,
      TargetPlatform.iOS,
    }),
  );

  testWidgets('dashboard deletion keys cannot remove shared panels', (
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
    var writes = 0;
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(platform: TargetPlatform.linux),
        home: Scaffold(
          body: BoardCanvas(
            panels: [panel('Form', 0, 0, 12, 8)],
            session: hub,
            interact: true,
            onChange: (_) => writes++,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final controller = tester
        .widget<grid.DashboardOverlay>(find.byType(grid.DashboardOverlay))
        .controller;
    for (final key in [
      LogicalKeyboardKey.delete,
      LogicalKeyboardKey.backspace,
    ]) {
      Focus.of(tester.element(find.text('Form'))).requestFocus();
      await tester.pump();
      await tester.sendKeyEvent(key);
      await tester.pumpAndSettle();
      expect(controller.layout.value.map((item) => item.id), ['Form']);
      expect(find.text('Form'), findsOneWidget);
    }
    await tester.enterText(find.byType(TextField), 'abc');
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    await tester.pumpAndSettle();
    expect(
      tester.widget<TextField>(find.byType(TextField)).controller!.text,
      'ab',
    );
    expect(controller.layout.value, hasLength(1));
    expect(writes, 0);
    await tester.pumpWidget(const SizedBox());
  }, variant: TargetPlatformVariant({TargetPlatform.linux}));

  for (final startWidth in [1100.0, 600.0]) {
    testWidgets('offscreen GenUI survives breakpoint from $startWidth', (
      tester,
    ) async {
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final hub = HubSession();
      addTearDown(hub.dispose);
      for (final message in demoMessages()) {
        hub.receive(message);
      }
      final panels = [
        for (var i = 0; i < 5; i++) panel('Form$i', 0, i * 8, 12, 8),
      ];
      var writes = 0;
      Future<void> show(double width, {bool remove = false}) async {
        tester.view.physicalSize = Size(width, 900);
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: BoardCanvas(
                panels: remove ? panels.take(3).toList() : panels,
                session: hub,
                interact: true,
                onChange: (_) => writes++,
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
      }

      Future<void> scrollTo(double offset) async {
        final scrollable = find
            .descendant(
              of: find.byWidgetPredicate(
                (widget) => widget is CustomScrollView,
              ),
              matching: find.byType(Scrollable),
            )
            .first;
        tester.state<ScrollableState>(scrollable).position.jumpTo(offset);
        await tester.pumpAndSettle();
      }

      await show(startWidth);
      await scrollTo(1300);
      final input = find.byType(TextField).hitTestable().last;
      await tester.enterText(input, 'Offscreen retained');
      final state = tester.state(input);
      final focus = tester
          .widget<EditableText>(
            find.descendant(of: input, matching: find.byType(EditableText)),
          )
          .focusNode;
      FocusManager.instance.primaryFocus?.unfocus();
      await tester.pump();
      await scrollTo(0);
      expect(focus.canRequestFocus, false);
      await show(startWidth == 1100 ? 600 : 1100);
      expect(state.mounted, true);
      await scrollTo(1300);
      expect(
        find
            .byType(TextField)
            .evaluate()
            .any((e) => identical(e, state.context)),
        true,
      );
      expect(
        (state.widget as TextField).controller!.text,
        'Offscreen retained',
      );
      expect(writes, 0);
      await show(startWidth == 1100 ? 600 : 1100, remove: true);
      expect(state.mounted, false);
      final otherState = tester.state(find.byType(TextField).first);
      await tester.pumpWidget(const SizedBox());
      expect(otherState.mounted, false);
    });
  }

  testWidgets('a replacement renderer releases retained input state', (
    tester,
  ) async {
    final hub = HubSession();
    addTearDown(hub.dispose);
    for (final message in demoMessages()) {
      hub.receive(message);
    }
    const replacementId = '$formId-replacement';
    for (final message in surfaceMessages(
      replacementId,
      [
        {
          'id': 'root',
          'component': 'TextField',
          'label': 'Replacement',
          'value': {'path': '/value'},
        },
      ],
      {'value': 'Fresh'},
    )) {
      hub.receive(message);
    }
    Future<void> show(String surfaceId) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: BoardCanvas(
              panels: [
                BoardPanel(
                  id: 'Form',
                  title: 'Form',
                  reference: SurfaceReference.fromSurfaceId(surfaceId),
                  x: 0,
                  y: 0,
                  w: 12,
                  h: 8,
                ),
              ],
              session: hub,
              interact: true,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    await show(formId);
    await tester.enterText(find.byType(TextField), 'Original state');
    final state = tester.state(find.byType(TextField));
    await show(replacementId);
    expect(state.mounted, false);
    expect(find.text('Fresh'), findsOneWidget);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('wide touch handle returns after pending save fails', (
    tester,
  ) async {
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.resetPhysicalSize);
    addTearDown(tester.view.resetDevicePixelRatio);
    final hub = HubSession();
    addTearDown(hub.dispose);
    final changes = <List<BoardPanel>>[];
    Future<void> show(double width, bool editing) async {
      tester.view.physicalSize = Size(width, 900);
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(platform: TargetPlatform.android),
          home: Scaffold(
            body: BoardCanvas(
              panels: [panel('a', 0, 0, 6, 4), panel('b', 6, 0, 6, 4)],
              session: hub,
              interact: false,
              onChange: editing ? changes.add : null,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    await show(600, true);
    await show(600, false);
    await show(1100, false);
    await show(1100, true);
    expect(find.byIcon(Icons.drag_handle), findsNWidgets(2));
    await tester.drag(
      find.byIcon(Icons.drag_handle).first,
      const Offset(0, 224),
    );
    await tester.pumpAndSettle();
    expect(changes, isNotEmpty);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('touch handle reorders narrow cards', (tester) async {
    final hub = HubSession();
    addTearDown(hub.dispose);
    List<BoardPanel>? changed;
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(platform: TargetPlatform.android),
        home: Scaffold(
          body: SizedBox(
            width: 600,
            child: BoardCanvas(
              panels: [panel('a', 0, 0, 6, 4), panel('b', 6, 0, 6, 4)],
              session: hub,
              interact: false,
              onChange: (next) => changed = next,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final gesture = await tester.startGesture(
      tester.getCenter(find.byIcon(Icons.drag_handle).first),
    );
    await tester.pump(const Duration(milliseconds: 700));
    await gesture.moveBy(const Offset(0, 1));
    await tester.pump();
    await gesture.moveBy(const Offset(0, 220));
    await tester.pump(const Duration(milliseconds: 500));
    await gesture.up();
    await tester.pumpAndSettle();
    expect(changed, isNotNull);
    expect(readingOrder(changed!).map((p) => p.id), ['b', 'a']);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('size sheet is staged until Apply and keeps canonical metadata', (
    tester,
  ) async {
    final hub = HubSession();
    addTearDown(hub.dispose);
    List<BoardPanel>? changed;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 600,
            child: BoardCanvas(
              panels: [panel('a', 0, 0, 12, 4)],
              session: hub,
              interact: false,
              onChange: (next) => changed = next,
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Size'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Compact'));
    await tester.pump();
    expect(changed, isNull);
    await tester.tap(find.text('Apply'));
    await tester.pumpAndSettle();
    expect(changed!.single.w, 4);
    expect(changed!.single.id, 'a');
    expect(changed!.single.reference.id, formId);
    await tester.pumpWidget(const SizedBox());
  });
}
