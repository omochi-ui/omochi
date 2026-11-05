import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/l10n/generated/app_messages.dart';
import 'package:omochi_mobile/src/remote/controller.dart';
import 'package:omochi_mobile/src/remote/models.dart';
import 'package:omochi_mobile/src/remote_connections.dart';

import 'remote_navigation_test.dart' show SelectionRepository, source;

class ConnectionRepository extends SelectionRepository {
  List<RemoteSource> records = [source];
  final forgotten = <String>[];
  @override
  Future<List<RemoteSource>> list() async => records;
  @override
  Future<void> forget(String id) async {
    forgotten.add(id);
    records = records.where((record) => record.id != id).toList();
  }

  @override
  Future<RemoteSource> parse(String invitation) async =>
      throw const FormatException('Invalid invitation');
}

Widget connections(RemoteConnectionsController controller, Locale locale) =>
    MaterialApp(
      locale: locale,
      localizationsDelegates: AppMessages.localizationsDelegates,
      supportedLocales: AppMessages.supportedLocales,
      home: RemoteConnections(controller: controller),
    );

void main() {
  testWidgets('connection catalogue and deletion use the injected repository', (
    tester,
  ) async {
    final repository = ConnectionRepository();
    final controller = RemoteConnectionsController(repository);
    addTearDown(controller.dispose);
    await tester.pumpWidget(connections(controller, const Locale('en')));
    await tester.pumpAndSettle();
    expect(find.text('Owner access · hub'), findsOneWidget);
    expect(find.text('grant · Pending'), findsOneWidget);
    await tester.tap(find.byIcon(Icons.delete_outline));
    await tester.pumpAndSettle();
    expect(repository.forgotten, ['hub/grant']);
    expect(find.text('Owner access · hub'), findsNothing);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('connection labels follow ARB locale changes', (tester) async {
    final controller = RemoteConnectionsController(ConnectionRepository());
    addTearDown(controller.dispose);
    await tester.pumpWidget(connections(controller, const Locale('en')));
    await tester.pumpAndSettle();
    expect(find.text('Hub connections'), findsOneWidget);
    await tester.pumpWidget(connections(controller, const Locale('ja')));
    await tester.pumpAndSettle();
    expect(find.text('Hub接続'), findsOneWidget);
    expect(find.text('grant · 承認待ち'), findsOneWidget);
    expect(find.text('オーナー接続 · hub'), findsOneWidget);
    expect(find.text('Hub connections'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets(
    'invitation errors remain visible and unlock connection actions',
    (tester) async {
      final controller = RemoteConnectionsController(ConnectionRepository());
      addTearDown(controller.dispose);
      await tester.pumpWidget(connections(controller, const Locale('en')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add Hub connection'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byType(TextField), 'invalid');
      await tester.tap(find.text('Continue'));
      await tester.pumpAndSettle();
      expect(find.text('Invalid invitation'), findsOneWidget);
      expect(controller.busy, isFalse);
      await tester.tap(find.text('Add Hub connection'));
      await tester.pumpAndSettle();
      expect(find.text('Connect to a Hub'), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
