import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/remote_connections.dart';

import 'invitation_flow_test.dart' show PairClient;
import 'remote_client_test.dart' show Repository, Transport;
import 'scoped_renderer_test.dart' show TestStore;

class LocalClient extends PairClient {
  int resumes = 0;
  @override
  Future<void> resume() async => resumes++;
}

Future<({StreamController<Uri> links, LocalClient local, Transport remote})>
start(WidgetTester tester) async {
  final links = StreamController<Uri>.broadcast();
  final local = LocalClient();
  final remote = Transport();
  final hub = HubSession(
    nativeStore: TestStore(),
    gatewayClientFactory:
        ({
          required store,
          required onMessage,
          required onRetract,
          required onEvent,
        }) {
          local.onMessage = onMessage;
          local.onRetract = onRetract;
          return local;
        },
  );
  await tester.pumpWidget(
    MaterialApp(
      home: MobileHome(
        links: links.stream,
        session: hub,
        remoteRepository: Repository(remote),
      ),
    ),
  );
  await tester.pumpAndSettle();
  await tester.tap(find.byIcon(Icons.hub_outlined));
  await tester.pumpAndSettle();
  addTearDown(links.close);
  return (links: links, local: local, remote: remote);
}

Uri gatewayInvitation() {
  final fixture = jsonDecode(
    File('../../spec/fixtures/gateway-invitations.json').readAsStringSync(),
  );
  return Uri.parse(fixture['cases'][0]['input']);
}

void main() {
  for (final link in ['omochi://share/example', 'omochi://owner/example']) {
    testWidgets('warm $link reaches the active chooser', (tester) async {
      final state = await start(tester);
      state.links.add(Uri.parse(link));
      await tester.pumpAndSettle();
      expect(find.byType(RemoteConnections), findsOneWidget);
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(find.widgetWithText(TextField, link), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
    });
  }

  testWidgets('remote Hub links return to the existing chooser', (
    tester,
  ) async {
    final state = await start(tester);
    await tester.tap(find.text('Owner access · hub'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.dashboard_outlined), findsOneWidget);
    state.links.add(Uri.parse('omochi://share/another'));
    await tester.pumpAndSettle();
    expect(state.remote.closes, greaterThan(0));
    expect(state.local.resumes, 0);
    expect(find.byType(RemoteConnections), findsOneWidget);
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(
      find.widgetWithText(TextField, 'omochi://share/another'),
      findsOneWidget,
    );
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });

  testWidgets('a warm link can replace a Hub opened from an invitation', (
    tester,
  ) async {
    final state = await start(tester);
    state.links.add(Uri.parse('omochi://owner/first'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Continue'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save connection'));
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.dashboard_outlined), findsOneWidget);
    state.links.add(Uri.parse('omochi://owner/second'));
    await tester.pumpAndSettle();
    expect(
      find.widgetWithText(TextField, 'omochi://owner/second'),
      findsOneWidget,
    );
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });

  testWidgets('gateway links leave the chooser and confirm on the local Hub', (
    tester,
  ) async {
    final state = await start(tester);
    final invitation = gatewayInvitation();
    state.links.add(invitation);
    await tester.pumpAndSettle();
    expect(find.byType(RemoteConnections), findsNothing);
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(state.local.resumes, 1);
    await tester.tap(find.text('Add to this device'));
    await tester.pumpAndSettle();
    expect(state.local.paired, [invitation.toString()]);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });

  testWidgets(
    'gateway enrollment waits for remote shutdown and pairs locally once',
    (tester) async {
      final state = await start(tester);
      await tester.tap(find.text('Owner access · hub'));
      await tester.pumpAndSettle();
      final remoteHome = tester.widget<MobileHome>(
        find.byType(MobileHome).last,
      );
      var guards = 0;
      remoteHome.session!.remoteLeaveGuard = () async {
        guards++;
        return true;
      };
      final closed = Completer<void>();
      state.remote.finishClose = closed;
      final invitation = gatewayInvitation();
      state.links.add(invitation);
      state.links.add(invitation);
      await tester.pumpAndSettle();
      expect(guards, 1);
      expect(find.byType(AlertDialog), findsNothing);
      expect(state.local.resumes, 0);
      closed.complete();
      await tester.pumpAndSettle();
      expect(state.local.resumes, 1);
      expect(find.byType(AlertDialog), findsOneWidget);
      expect(find.byType(RemoteConnections), findsNothing);
      await tester.tap(find.text('Add to this device'));
      await tester.pumpAndSettle();
      expect(state.local.paired, [invitation.toString()]);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pumpAndSettle();
    },
  );

  testWidgets('declining the remote draft guard retains its session', (
    tester,
  ) async {
    final state = await start(tester);
    await tester.tap(find.text('Owner access · hub'));
    await tester.pumpAndSettle();
    final remoteHome = tester.widget<MobileHome>(find.byType(MobileHome).last);
    remoteHome.session!.remoteLeaveGuard = () async => false;
    state.links.add(gatewayInvitation());
    await tester.pumpAndSettle();
    expect(find.byIcon(Icons.dashboard_outlined), findsOneWidget);
    expect(find.byType(AlertDialog), findsNothing);
    expect(state.remote.closes, 0);
    expect(state.local.resumes, 0);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });
}
