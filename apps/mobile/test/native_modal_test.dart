import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/models.dart';

import 'mobile_rendering_test.dart' show openService, session;
import 'native_layout_test.dart' show update;
import 'scoped_renderer_test.dart' show TestClient, TestScope;
import 'support/renderer_fixture.dart';

List<Json> modal(String title) => [
  {'id': 'root', 'component': 'Modal', 'trigger': 'open', 'content': 'body'},
  textComponent('open', 'Open $title'),
  textComponent('body', 'Sheet $title'),
];

VoidCallback openCallback(WidgetTester tester, String title) => tester
    .widget<ElevatedButton>(
      find.ancestor(
        of: find.text('Open $title'),
        matching: find.byType(ElevatedButton),
      ),
    )
    .onPressed!;

void main() {
  testWidgets(
    'open modal follows text, action and content definition updates',
    (tester) async {
      final client = TestClient();
      final hub = session(client);
      final scope = TestScope('modal:update');
      await openService(tester, hub, client, scope);
      List<Json> definition(String title, String content) => [
        {
          'id': 'root',
          'component': 'Modal',
          'trigger': 'open',
          'content': content,
        },
        textComponent('open', 'Open'),
        {
          'id': content,
          'component': 'Column',
          'children': ['text', 'action'],
        },
        textComponent('text', title),
        buttonComponent('action', 'label', title),
        textComponent('label', 'Submit'),
      ];
      update(client, definition('Before', 'body'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      final sheet = tester.element(find.byType(BottomSheet));
      update(client, definition('After', 'body'));
      await tester.pumpAndSettle();
      expect(find.text('Before'), findsNothing);
      expect(find.text('After'), findsOneWidget);
      expect(tester.element(find.byType(BottomSheet)), same(sheet));
      await tester.tap(find.text('Submit'));
      await tester.pumpAndSettle();
      expect(scope.actions.single['action']['name'], 'After');
      update(client, definition('Replacement', 'new-body'));
      await tester.pumpAndSettle();
      expect(find.text('Replacement'), findsOneWidget);
      expect(find.text('After'), findsNothing);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    },
  );

  for (final operation in [
    'delete',
    'replace',
    'restart',
    'remove component',
    'disconnect',
    'forget',
    'suspend',
    'stop',
    'leave',
    'dispose',
  ]) {
    testWidgets('Modal closes on $operation and expires captured callback', (
      tester,
    ) async {
      final client = TestClient();
      final hub = session(client);
      await openService(tester, hub, client, TestScope('modal:old'));
      update(client, modal('old'));
      await tester.pumpAndSettle();
      final staleOpen = openCallback(tester, 'old');
      await tester.tap(find.text('Open old'));
      await tester.pumpAndSettle();
      expect(find.text('Sheet old'), findsOneWidget);
      switch (operation) {
        case 'delete':
          client.onMessage({
            'version': 'v0.9',
            'deleteSurface': {'surfaceId': counterId},
          });
        case 'replace':
          client.show(TestScope('modal:new'));
          update(client, modal('new'));
        case 'restart':
          await hub.startNative();
          client.show(TestScope('modal:old')); // Same ID, different renderer.
          update(client, modal('new'));
        case 'remove component':
          update(client, [textComponent('root', 'No modal')]);
        case 'disconnect':
          client.onRetract('$demoNode~playground');
        case 'forget':
          await hub.forgetGateway(demoNode);
        case 'suspend':
          await hub.suspendNative();
        case 'stop':
          await hub.stopNative();
        case 'leave':
          // Switch the underlying service screen while its sheet is open.
          tester
              .widget<TextButton>(
                find.widgetWithText(TextButton, 'Apps', skipOffstage: false),
              )
              .onPressed!();
        case 'dispose':
          // Widget disposal occurs during the frame, while Navigator is locked.
          await tester.pumpWidget(const SizedBox());
      }
      if (operation != 'leave' && operation != 'remove component') {
        staleOpen(); // Before the old widget is unmounted.
      }
      await tester.pumpAndSettle();
      expect(find.text('Sheet old'), findsNothing);
      staleOpen();
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet), findsNothing);
      expect(tester.takeException(), isNull);
      if (operation == 'replace' || operation == 'restart') {
        await tester.tap(find.text('Open new'));
        await tester.pumpAndSettle();
        expect(find.text('Sheet new'), findsOneWidget);
        staleOpen();
        await tester.pumpAndSettle();
        expect(find.text('Sheet new'), findsOneWidget);
      }
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('Modal can reopen after normal dismiss', (tester) async {
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('modal:dismiss'));
    update(client, modal('dismiss'));
    await tester.pumpAndSettle();
    for (var i = 0; i < 3; i++) {
      await tester.tap(find.text('Open dismiss'));
      await tester.pumpAndSettle();
      expect(find.text('Sheet dismiss'), findsOneWidget);
      Navigator.of(tester.element(find.text('Sheet dismiss'))).pop();
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet), findsNothing);
    }
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('a pending sheet never builds content for an expired Surface', (
    tester,
  ) async {
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('modal:pending'));
    update(client, modal('pending'));
    await tester.pumpAndSettle();
    openCallback(tester, 'pending')();
    // No pump between route creation and renderer disposal.
    await hub.stopNative();
    await tester.pumpAndSettle();
    expect(find.byType(BottomSheet), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('Surface deletion removes its sheet underneath another route', (
    tester,
  ) async {
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('modal:dialog'));
    update(client, modal('dialog'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open dialog'));
    await tester.pumpAndSettle();
    final dialog = showDialog<void>(
      context: tester.element(find.text('Sheet dialog')),
      builder: (_) => const AlertDialog(title: Text('Keep this dialog')),
    );
    await tester.pumpAndSettle();
    client.onMessage({
      'version': 'v0.9',
      'deleteSurface': {'surfaceId': counterId},
    });
    await tester.pumpAndSettle();
    expect(find.byType(BottomSheet, skipOffstage: false), findsNothing);
    expect(find.text('Keep this dialog'), findsOneWidget);
    Navigator.of(tester.element(find.text('Keep this dialog'))).pop();
    await dialog;
    await tester.pumpAndSettle();
    expect(find.text('Apps'), findsWidgets);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'each Surface owns only its routes, even with the same Modal ID',
    (tester) async {
      final hub = HubSession();
      for (final id in ['a', 'b']) {
        for (final message in surfaceMessages(id, modal(id), {})) {
          hub.receive(message);
        }
      }
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Row(
              children: [
                for (final id in ['a', 'b'])
                  Expanded(
                    child: Surface(surfaceContext: hub.contextForSurface(id)),
                  ),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final openA = openCallback(tester, 'a');
      final openB = openCallback(tester, 'b');
      openA();
      await tester.pumpAndSettle();
      openB();
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet, skipOffstage: false), findsNWidgets(2));
      hub.receive({
        'version': 'v0.9',
        'deleteSurface': {'surfaceId': 'a'},
      });
      openA();
      await tester.pumpAndSettle();
      expect(find.byType(BottomSheet, skipOffstage: false), findsOneWidget);
      expect(find.text('Sheet b'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      hub.dispose();
      await tester.pumpAndSettle();
    },
  );
}
