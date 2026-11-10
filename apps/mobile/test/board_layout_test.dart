import 'package:flutter/services.dart';

import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter/material.dart';
import 'package:omochi_mobile/src/remote/board_layout.dart';
import 'package:omochi_mobile/src/remote/board_models.dart';
import 'package:omochi_mobile/src/remote/board_canvas.dart';
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
      await tester.drag(find.byType(CustomScrollView), const Offset(0, -1800));
      await tester.pumpAndSettle();
      await tester.drag(find.byType(CustomScrollView), const Offset(0, 2500));
      await tester.pumpAndSettle();
      expect(tester.state(find.byType(TextField).first), same(state));
      await show(1100);
      expect(tester.state(find.byType(TextField).first), same(state));
      expect(writes, 0);
      await tester.pumpWidget(const SizedBox());
    },
  );
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
