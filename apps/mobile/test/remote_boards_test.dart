import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/models.dart';
import 'package:omochi_mobile/src/native/client_remote.dart';
import 'package:omochi_mobile/src/native/store.dart';
import 'package:omochi_mobile/src/remote_boards.dart';

class BoardClient extends RemoteClient {
  BoardClient(String mode)
    : super(NativeHubStore(), {'mode': mode}, (_) {}, (_) {}, (_) {});
  final commands = <Json>[];
  bool conflict = false;
  @override
  Future<dynamic> control(Json value) async {
    commands.add(value);
    if (conflict) throw StateError('Conflict');
    return <Json>[];
  }

  @override
  Future<Json> rpc(Json value) async {
    commands.add(value);
    if (conflict) throw StateError('Conflict');
    return {'kind': 'board'};
  }
}

Json board() => {
  'id': 'board',
  'revision': 1,
  'name': 'Counter board',
  'panels': <Json>[
    {
      'id': 'counter',
      'title': 'Counter',
      'reference': {
        'nodeId': 'a' * 64,
        'serviceId': 'counter',
        'surfaceId': 'main',
      },
      'x': 0,
      'y': 0,
      'w': 12,
      'h': 4,
    },
  ],
};
void main() {
  testWidgets(
    'shared layout editor cannot change membership and preserves conflicting draft',
    (tester) async {
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final client = BoardClient('invite');
      client.welcome = {
        'permissions': {'interact': false, 'editLayout': true},
      };
      client.conflict = true;
      final session = HubSession();
      addTearDown(session.dispose);
      final screenshotKey = GlobalKey();
      await tester.pumpWidget(
        RepaintBoundary(
          key: screenshotKey,
          child: MaterialApp(
            home: BoardEditor(client: client, session: session, board: board()),
          ),
        ),
      );
      expect(find.text('Add panel'), findsNothing);
      expect(find.text('Remove panel'), findsNothing);
      await tester.enterText(find.byType(TextFormField).first, 'My draft');
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Conflict'), findsOneWidget);
      expect(find.text('My draft'), findsOneWidget);
      expect(client.commands.single['op'], 'layout');
      expect(client.commands.single['value']['revision'], 1);
      expect(tester.takeException(), isNull);
      const screenshot = String.fromEnvironment('REMOTE_BOARDS_SCREENSHOT');
      if (screenshot.isNotEmpty) {
        final boundary =
            screenshotKey.currentContext!.findRenderObject()
                as RenderRepaintBoundary;
        await tester.runAsync(() async {
          final image = await boundary.toImage();
          try {
            final data = await image.toByteData(format: ui.ImageByteFormat.png);
            await File(screenshot).writeAsBytes(data!.buffer.asUint8List());
          } finally {
            image.dispose();
          }
        });
      }
    },
  );
  testWidgets(
    'owner editor exposes panel membership and guards unsaved navigation',
    (tester) async {
      final client = BoardClient('owner');
      final session = HubSession();
      addTearDown(session.dispose);
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => BoardEditor(
                    client: client,
                    session: session,
                    board: board(),
                  ),
                ),
              ),
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      expect(find.text('Add panel'), findsOneWidget);
      expect(find.text('Remove panel'), findsOneWidget);
      await tester.enterText(find.byType(TextFormField).first, 'Unsaved');
      await tester.pump();
      await tester.pageBack();
      await tester.pumpAndSettle();
      expect(find.text('Discard unsaved changes?'), findsOneWidget);
      await tester.tap(find.text('Keep editing'));
      await tester.pumpAndSettle();
      expect(find.text('Unsaved'), findsOneWidget);
      expect(client.commands, isEmpty);
      expect(session.remoteLeaveGuard, isNotNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
      expect(session.remoteLeaveGuard, isNull);
    },
  );
}
