import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:integration_test/integration_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/native/store.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const url = String.fromEnvironment(
    'FIXTURE_URL',
    defaultValue: 'http://127.0.0.1:5559',
  );
  testWidgets(
    'pair two gateways, render, act, resume, restart, reconnect and forget',
    (tester) async {
      final store = NativeHubStore(
        namespace:
            'omochi.test.mobile.${DateTime.now().microsecondsSinceEpoch}',
      );
      await store.setOnline(
        const bool.fromEnvironment('TEST_ONLINE', defaultValue: true),
      );
      final hub = HubSession(nativeStore: store);
      addTearDown(() async {
        await hub.stopNative();
        await store.clearTestNamespace();
      });
      Future<void> until(bool Function() check, String reason) async {
        final deadline = DateTime.now().add(const Duration(seconds: 90));
        while (!check() && DateTime.now().isBefore(deadline)) {
          await tester.pump(const Duration(milliseconds: 100));
        }
        expect(
          check(),
          true,
          reason: '$reason (${hub.status}: ${hub.error}; ${hub.connections})',
        );
        debugPrint('Verified: $reason');
      }

      Future<Map<String, dynamic>> fixture(String path) async {
        final response = await http
            .post(Uri.parse('$url$path'))
            .timeout(const Duration(seconds: 40));
        expect(response.statusCode, 200);
        return jsonDecode(response.body) as Map<String, dynamic>;
      }

      final data = await fixture('/start');
      await tester.pumpWidget(
        MaterialApp(
          home: MobileHome(links: const Stream.empty(), session: hub),
        ),
      );
      await until(
        () => hub.nativeIdentity != null,
        'secure identity initialized',
      );
      final identity = hub.nativeIdentity;
      await hub.resumeNative();
      await hub.resumeNative();
      expect(
        hub.error,
        isNull,
        reason: 'duplicate resumed notification is not a rendering error',
      );
      for (final invite in data['invitations'] as List) {
        await hub.pairGateway(invite as String);
      }
      await until(
        () => hub.surfaceIds.length == 2 && hub.services.length == 2,
        'two scoped surfaces',
      );
      expect(hub.services.length, 2);
      await tester.tap(find.text('main').first);
      await tester.pump(const Duration(seconds: 1));
      await until(
        () => find.text('Run').evaluate().isNotEmpty,
        'rendered action',
      );
      await tester.tap(find.text('Run'));
      await until(
        () => find.textContaining('Ran at temperature').evaluate().isNotEmpty,
        'action returned to SDK',
      );
      await hub.suspendNative();
      expect(hub.surfaceIds, isEmpty);
      await hub.resumeNative();
      await until(() => hub.surfaceIds.length == 2, 'resume snapshot');
      await hub.stopNative();
      await hub.startNative();
      expect(hub.nativeIdentity, identity);
      await until(
        () => hub.surfaceIds.length == 2,
        'restart retains enrollment',
      );
      await fixture('/service/0/start');
      await until(
        () => hub.surfaceIds.length == 2,
        'service generation recovered',
      );
      await fixture('/gateway/0/stop');
      await until(() => hub.surfaceIds.length == 1, 'one gateway offline');
      await fixture('/gateway/0/start');
      await until(() => hub.surfaceIds.length == 2, 'gateway reconnect');
      await hub.forgetGateway(hub.gateways.first.address.id);
      await until(
        () => hub.gateways.length == 1 && hub.surfaceIds.length == 1,
        'forget isolates gateway',
      );
      expect(await store.readIdentityDocument(), isNotNull);
      // Record actual process RSS; pixel/heap accounting is tested separately.
      debugPrint(
        'Mobile ${Platform.operatingSystem} RSS: ${ProcessInfo.currentRss} bytes',
      );
      await tester.pumpWidget(const SizedBox());
    },
  );
}
