import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/native/store.dart';

import '../test/support/memory_secure_storage.dart';
import 'support/bonjour.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const memory = bool.fromEnvironment('BONJOUR_MEMORY_STORE');
  testWidgets(
    'sandboxed macOS Hub resolves via OS Bonjour (test memory store: $memory)',
    (tester) async {
      final store = NativeHubStore(
        namespace:
            'omochi.test.bonjour.${DateTime.now().microsecondsSinceEpoch}',
        secure: memory ? MemorySecureStorage() : null,
      );
      await store.setOnline(false);
      // Surface the OS error before the Hub correctly treats a failed save as an
      // unknown outcome. This writes no identity material.
      final probeKey = '${store.namespace}.probe';
      await store.secure.write(key: probeKey, value: 'probe');
      expect(await store.secure.read(key: probeKey), 'probe');
      await store.secure.delete(key: probeKey);
      final hub = HubSession(nativeStore: store);
      addTearDown(() async {
        await hub.stopNative();
        await store.clearTestNamespace();
      });
      await tester.pumpWidget(
        MaterialApp(
          home: MobileHome(links: const Stream.empty(), session: hub),
        ),
      );
      final deadline = DateTime.now().add(const Duration(seconds: 30));
      while (hub.nativeIdentity == null && DateTime.now().isBefore(deadline)) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(hub.error, isNull);
      final identity = hub.nativeIdentity;
      expect(identity, matches(RegExp(r'^[a-f0-9]{64}$')));
      expect(await store.readIdentityDocument(), isNotNull);
      final instance = bonjourInstance(identity!);
      final record = await resolveBonjour(instance);
      expect(record.name, '$instance._irohv1._udp.local.');
      expect(record.port, greaterThan(0));
      expect(record.interfaceIndex, greaterThan(0));
      debugPrint('Bonjour Hub identity: $identity');
      debugPrint(
        'OS Bonjour resolved: ${record.host}:${record.port} interface=${record.interfaceIndex}',
      );
      const peer = String.fromEnvironment('BONJOUR_PEER_ID');
      if (peer.isNotEmpty) {
        final peerInstance = bonjourInstance(peer);
        final remote = await resolveBonjour(peerInstance);
        expect(remote.name, '$peerInstance._irohv1._udp.local.');
        expect(remote.port, greaterThan(0));
        debugPrint(
          'LAN peer resolved via OS Bonjour: ${remote.host}:${remote.port}',
        );
      }
      const hold = int.fromEnvironment('BONJOUR_HOLD_SECONDS');
      if (hold > 0) {
        await Future<void>.delayed(Duration(seconds: hold));
      }
      await hub.stopNative();
      await hub.startNative();
      expect(hub.nativeIdentity, identity);
      expect(hub.error, isNull);
      await hub.stopNative();
      await tester.pumpWidget(const SizedBox());
    },
    skip: !Platform.isMacOS,
  );
}
