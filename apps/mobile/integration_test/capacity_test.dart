import 'dart:convert';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:http/io_client.dart';
import 'package:integration_test/integration_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/messages.dart';
import 'package:omochi_mobile/src/native/image_budget.dart';
import 'package:omochi_mobile/src/native/store.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const url = String.fromEnvironment(
    'FIXTURE_URL',
    defaultValue: 'http://127.0.0.1:5559',
  );
  testWidgets('eight services keep bounded images and reject only the ninth', (
    tester,
  ) async {
    const fixtureCa = String.fromEnvironment('FIXTURE_CA_BASE64');
    final client = IOClient(
      HttpClient(
        context: fixtureCa.isEmpty
            ? null
            : (SecurityContext(withTrustedRoots: false)
                ..setTrustedCertificatesBytes(base64Decode(fixtureCa))),
      ),
    );
    addTearDown(client.close);
    final store = NativeHubStore(
      namespace:
          'omochi.test.capacity.${DateTime.now().microsecondsSinceEpoch}',
    );
    await store.setOnline(true);
    final hub = HubSession(nativeStore: store);
    final samples = <Map<String, Object>>[];
    void sample(String phase) {
      samples.add({
        'phase': phase,
        'rssBytes': ProcessInfo.currentRss,
        'decodedImageBytes': NativeImageBudget.shared.decodedBytes,
        'imageLoads': NativeImageBudget.shared.loading,
      });
    }

    addTearDown(() async {
      await hub.stopNative();
      hub.dispose();
      await store.clearTestNamespace();
    });
    Future<void> until(bool Function() check, String reason) async {
      for (var i = 0; i < 900 && !check(); i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(check(), true, reason: '$reason: ${hub.error}');
    }

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AnimatedBuilder(
            animation: hub,
            builder: (context, _) => SingleChildScrollView(
              child: Column(
                children: [
                  for (final id in hub.surfaceIds)
                    Surface(
                      key: ValueKey(hub.contextForSurface(id).surfaceId),
                      surfaceContext: hub.contextForSurface(id),
                    ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
    await hub.startNative();
    sample('started');
    final response = await client.post(
      Uri.parse('$url/start'),
      body: jsonEncode({'services': 4, 'imageSize': 1024, 'modelBytes': 65536}),
    );
    expect(response.statusCode, 200);
    for (final invitation in jsonDecode(response.body)['invitations']) {
      await hub.pairGateway(invitation as String);
    }
    await until(
      () => hub.surfaceIds.length == 8 && hub.services.length == 8,
      'eight services',
    );
    for (
      var i = 0;
      i < 200 && NativeImageBudget.shared.decodedBytes < 32 * 1024 * 1024;
      i++
    ) {
      await tester.pump(const Duration(milliseconds: 100));
      final retry = find.widgetWithText(TextButton, messages.retryImage);
      if (retry.evaluate().isNotEmpty &&
          NativeImageBudget.shared.loading == 0) {
        await tester.ensureVisible(retry.first);
        await tester.tap(retry.first);
      }
    }
    expect(NativeImageBudget.shared.decodedBytes, 32 * 1024 * 1024);
    sample('eight-services-images');
    await client.post(Uri.parse('$url/service/0/extra'));
    await until(() => hub.services.length == 9, 'ninth reported');
    expect(hub.surfaceIds.length, 8);
    expect(
      hub.services.where((service) => service.status == 'error'),
      hasLength(1),
    );
    await tester.ensureVisible(find.text('Run').first);
    await tester.tap(find.text('Run').first);
    await until(
      () => find.textContaining('Ran at temperature').evaluate().isNotEmpty,
      'healthy action after capacity rejection',
    );
    await hub.suspendNative();
    await tester.pump(const Duration(seconds: 1));
    expect(NativeImageBudget.shared.decodedBytes, 0);
    expect(NativeImageBudget.shared.loading, 0);
    sample('suspended');
    await hub.stopNative();
    sample('stopped');
    binding.reportData = {
      'platform': Platform.operatingSystem,
      'mode': kReleaseMode ? 'release' : 'debug',
      'gateways': 2,
      'services': 8,
      'samples': samples,
    };
    debugPrint('Capacity memory: ${jsonEncode(binding.reportData)}');
    await tester.pumpWidget(const SizedBox());
  });
}
