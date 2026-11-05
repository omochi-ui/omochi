import 'dart:async';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/rendering.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/models.dart';
import 'package:omochi_mobile/src/native/client_remote.dart';
import 'package:omochi_mobile/src/native/store.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/remote/models.dart';
import 'package:omochi_mobile/src/remote/board_models.dart';
import 'package:omochi_mobile/src/remote/boards_repository.dart';
import 'package:omochi_mobile/src/remote/boards_controller.dart';
import 'package:omochi_mobile/l10n/generated/app_messages.dart';
import 'package:omochi_mobile/src/remote_boards.dart';

class BoardClient extends RemoteClient {
  BoardClient(String mode)
    : super(
        NativeHubStore(),
        RemoteSource(
          id: 'source',
          hubId: 'hub',
          grantId: 'grant',
          mode: RemoteAccessMode.values.byName(mode),
        ),
        (_) {},
        (_) {},
        (_) {},
      );
  final commands = <Json>[];
  bool conflict = false;
  BridgeException? failure;
  RemoteBoard? latest;
  Future<Object?>? response;
  @override
  Future<dynamic> control(Json value) async {
    commands.add(value);
    if (conflict) throw BridgeException('Conflict', 'Conflict');
    if (failure != null) throw failure!;
    if (response != null) return response!;
    if (value['method'] == 'boardGet') return latest!.toJson();
    if (value['method'] == 'shareCreate') return {'url': 'omochi://share/link'};
    return <Json>[];
  }

  @override
  Future<Json> rpc(Json value) async {
    commands.add(value);
    if (conflict) throw BridgeException('Conflict', 'Conflict');
    if (failure != null) throw failure!;
    return {'kind': 'board', 'board': latest?.toJson() ?? board().toJson()};
  }
}

RemoteBoard board() => RemoteBoard.fromJson({
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
});
void main() {
  test(
    'draft remains independent and only structured conflict enables reload',
    () async {
      final client = BoardClient('invite');
      final original = board();
      final controller = BoardEditorController(
        RemoteBoardsRepository(client),
        original,
      );
      addTearDown(controller.dispose);
      controller.rename('My draft');
      controller.renamePanel(controller.panels.single, 'My panel');
      controller.resizePanel(
        controller.panels.single,
        PanelDimension.width,
        '6',
      );
      controller.removePanel(controller.panels.single);
      controller.addPanel('node~service/surface');
      expect(controller.panels, hasLength(1));
      expect(original.name, 'Counter board');
      expect(original.panels.single.title, 'Counter');
      expect(original.panels.single.w, 12);
      client.failure = BridgeException(
        'NetworkUnavailable',
        'Conflict appears in diagnostics',
      );
      expect(await controller.save(), false);
      expect(controller.conflict, false);
      expect(controller.dirty, true);
      client.failure = BridgeException('Conflict', 'Changed elsewhere');
      expect(await controller.save(), false);
      expect(controller.conflict, true);
      expect(controller.name, 'My draft');
      expect(client.commands.last['value']['revision'], 1);
      expect(
        client.commands.last['value']['panels'].single['reference'],
        original.panels.single.reference.toJson(),
      );
    },
  );

  test(
    'explicit reload adopts latest revision and owner save uses that revision',
    () async {
      final client = BoardClient('owner');
      final controller = BoardEditorController(
        RemoteBoardsRepository(client),
        board(),
      );
      addTearDown(controller.dispose);
      controller.rename('stale draft');
      client.latest = RemoteBoard(
        id: 'board',
        revision: 3,
        name: 'Latest',
        panels: board().panels,
      );
      await controller.reload();
      expect(controller.name, 'Latest');
      expect(controller.revision, 3);
      expect(controller.dirty, false);
      controller.rename('Next');
      expect(await controller.save(), true);
      expect(client.commands.last['method'], 'boardUpdate');
      expect(client.commands.last['value']['revision'], 3);
    },
  );

  test('disposing a controller ignores an outstanding refresh', () async {
    final client = BoardClient('owner');
    final response = Completer<Object?>();
    client.response = response.future;
    final controller = BoardsController(RemoteBoardsRepository(client));
    var notifications = 0;
    controller.addListener(() => notifications++);
    final pending = controller.refresh();
    controller.dispose();
    response.complete([board().toJson()]);
    await pending;
    expect(notifications, 0);
    expect(controller.boards, isEmpty);
  });

  test(
    'public shares strip permissions and shared boards use welcome metadata',
    () async {
      final owner = BoardClient('owner');
      final repository = RemoteBoardsRepository(owner);
      expect(
        await repository.createShare(
          boardId: 'board',
          public: true,
          interact: true,
          editLayout: true,
        ),
        'omochi://share/link',
      );
      expect(owner.commands.single['permissions'], {
        'interact': false,
        'editLayout': false,
      });
      final client = BoardClient('public')
        ..welcome = RemoteWelcome(board: board());
      final shared = RemoteBoardsRepository(client);
      expect((await shared.list()).single.name, 'Counter board');
      expect((await shared.get('board')).revision, 1);
      expect(client.commands, isEmpty);
    },
  );

  test('shared save updates the board used by subsequent reads', () async {
    final client = BoardClient('invite')
      ..welcome = RemoteWelcome(board: board());
    client.latest = RemoteBoard(
      id: 'board',
      revision: 2,
      name: 'Saved',
      panels: board().panels,
    );
    final repository = RemoteBoardsRepository(client);
    await repository.save(client.latest!, create: false);
    expect((await repository.get('board')).revision, 2);
    expect((await repository.list()).single.name, 'Saved');
  });

  testWidgets('successful save leaves the editor without an unsaved dialog', (
    tester,
  ) async {
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
                  repository: RemoteBoardsRepository(client),
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
    await tester.enterText(find.byType(TextFormField).first, 'Saved');
    await tester.pump();
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.text('Open'), findsOneWidget);
    expect(find.text('Discard unsaved changes?'), findsNothing);
    expect(session.remoteLeaveGuard, isNull);
  });

  testWidgets('board editor uses ARB and follows locale changes', (
    tester,
  ) async {
    final client = BoardClient('owner');
    final session = HubSession();
    addTearDown(session.dispose);
    final locale = ValueNotifier(const Locale('en'));
    addTearDown(locale.dispose);
    await tester.pumpWidget(
      ValueListenableBuilder<Locale>(
        valueListenable: locale,
        builder: (context, value, child) => MaterialApp(
          locale: value,
          localizationsDelegates: AppMessages.localizationsDelegates,
          supportedLocales: AppMessages.supportedLocales,
          home: BoardEditor(
            repository: RemoteBoardsRepository(client),
            session: session,
            board: board(),
          ),
        ),
      ),
    );
    expect(find.text('Edit board'), findsOneWidget);
    locale.value = const Locale('ja');
    await tester.pumpAndSettle();
    expect(find.text('ボードを編集'), findsOneWidget);
    expect(find.text('保存'), findsOneWidget);
    expect(find.text('Add panel'), findsNothing);
    expect(find.text('パネルを追加'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'shared layout editor cannot change membership and preserves conflicting draft',
    (tester) async {
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final client = BoardClient('invite');
      client.welcome = const RemoteWelcome(
        permissions: RemotePermissions(editLayout: true),
      );
      client.conflict = true;
      final session = HubSession();
      addTearDown(session.dispose);
      final screenshotKey = GlobalKey();
      await tester.pumpWidget(
        RepaintBoundary(
          key: screenshotKey,
          child: MaterialApp(
            home: BoardEditor(
              repository: RemoteBoardsRepository(client),
              session: session,
              board: board(),
            ),
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
                    repository: RemoteBoardsRepository(client),
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
