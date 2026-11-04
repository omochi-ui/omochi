import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/native/client_shared.dart';
import 'package:omochi_mobile/src/native/store.dart';

import 'support/memory_secure_storage.dart';

class _DelayedPreferencesStore extends NativeHubStore {
  _DelayedPreferencesStore()
    : super(namespace: 'omochi.test.lifecycle', secure: MemorySecureStorage());
  final writeStarted = Completer<void>();
  final finishWrite = Completer<void>();
  @override
  Future<bool> online() async => false;
  @override
  Future<void> setOnline(bool value) async {
    writeStarted.complete();
    await finishWrite.future;
  }
}

class _DelayedLeaseStore extends NativeHubStore {
  final acquired = Completer<Future<void> Function()>();
  @override
  Future<Future<void> Function()> acquireIdentityLease() => acquired.future;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  if (Platform.isMacOS) {
    // Native flock behavior is exercised by test_driver/macos/identity_lease_test.py.
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(
          const MethodChannel('omochi/identity-lease'),
          (call) async => call.method == 'acquire' ? 'test-lease' : null,
        );
  }
  test(
    'closing during lease acquisition releases the lease without opening a Hub',
    () async {
      final store = _DelayedLeaseStore();
      var released = false;
      final client = createSharedGatewayClient(
        store: store,
        onMessage: (_) {},
        onRetract: (_) {},
        onEvent: (_) {},
        openBridge: (_) => throw StateError('must not open'),
      );
      final started = expectLater(client.start(), throwsStateError);
      await client.close();
      store.acquired.complete(() async {
        released = true;
      });
      await started;
      expect(released, isTrue);
    },
  );

  test('lease acquisition failure never opens a Hub', () async {
    final store = _DelayedLeaseStore();
    var opened = false;
    final client = createSharedGatewayClient(
      store: store,
      onMessage: (_) {},
      onRetract: (_) {},
      onEvent: (_) {},
      openBridge: (_) {
        opened = true;
        throw StateError('must not open');
      },
    );
    final started = expectLater(client.start(), throwsStateError);
    store.acquired.completeError(StateError('This Hub is already running'));
    await started;
    expect(opened, isFalse);
    await client.close();
  });

  final file = File(
    Platform.environment['OMOCHI_HUB_LIBRARY'] ??
        'build/shared-hub/linux/libomochi_native_bridge.so',
  );
  group(
    'shared Hub C ABI',
    skip: !file.existsSync()
        ? 'Run tool/build_shared_hub.py --platform linux --output build/shared-hub/linux'
        : false,
    () {
      SharedHubBridge open(MemorySecureStorage secure) => SharedHubBridge(
        NativeHubStore(namespace: 'omochi.test.bridge', secure: secure),
        library: DynamicLibrary.open(file.absolute.path),
      );
      for (final pauseDuringSave in [true, false]) {
        test(
          'relay preference preserves suspension (pause during save: $pauseDuringSave)',
          () async {
            final store = _DelayedPreferencesStore();
            late SharedHubBridge bridge;
            final client = createSharedGatewayClient(
              store: store,
              onMessage: (_) {},
              onRetract: (_) {},
              onEvent: (_) {},
              openBridge: (store) => bridge = SharedHubBridge(
                store,
                library: DynamicLibrary.open(file.absolute.path),
              ),
            );
            addTearDown(client.close);
            await client.start();
            if (!pauseDuringSave) await client.suspend();
            final setting = client.setOnline(false);
            await store.writeStarted.future;
            if (pauseDuringSave) await client.suspend();
            store.finishWrite.complete();
            await setting;
            await expectLater(
              bridge.request({'op': 'subscribe'}),
              throwsA(
                isA<StateError>().having(
                  (e) => e.toString(),
                  'message',
                  contains('suspended'),
                ),
              ),
            );
            await client.resume();
            // A deliberate foreground transition must restore rendering.
            await bridge.request({'op': 'subscribe'});
          },
        );
      }

      test('native snapshot ACK, cancellation, suspend and restart preserve the original key', () async {
        final secure = MemorySecureStorage();
        final bridge = open(secure);
        final start = await bridge.request({'op': 'start', 'online': false});
        final saved = secure.value;
        expect(secure.writes, 1);
        expect(jsonDecode(saved!)['peers'], isEmpty);
        await bridge.request({'op': 'subscribe'});
        final batch = await bridge.request({'op': 'next'});
        expect(batch['snapshotEnd'], true);
        final next = bridge.call({'op': 'next', 'ack': batch['sequence']});
        final cancelled = expectLater(next.result, throwsStateError);
        next.cancel();
        await cancelled;
        await bridge.request({'op': 'suspend'});
        await bridge.request({'op': 'resume', 'online': false});
        await bridge.close();
        final reopened = open(secure);
        final again = await reopened.request({'op': 'start', 'online': false});
        expect(again['identity'], start['identity']);
        expect(secure.value, saved);
        expect(secure.writes, 1);
        await reopened.close();
      });
      test('close keeps polling a dispatched save until it settles', () async {
        final secure = MemorySecureStorage()
          ..writeStarted = Completer<void>()
          ..finishWrite = Completer<void>();
        final bridge = open(secure);
        final start = bridge.request({'op': 'start', 'online': false});
        await secure.writeStarted!.future;
        var closed = false;
        final close = bridge.close().then((_) => closed = true);
        await Future<void>.delayed(const Duration(milliseconds: 30));
        expect(closed, false);
        secure.finishWrite!.complete();
        await start;
        await close;
        expect(secure.value, isNotNull);
      });
      test('corrupt and unavailable existing storage never creates a replacement key', () async {
        for (final value in ['{}', 'invalid', '{"secret":"bad","peers":[]}']) {
          final secure = MemorySecureStorage()..value = value;
          final bridge = open(secure);
          await expectLater(
            bridge.request({'op': 'start', 'online': false}),
            throwsStateError,
          );
          expect(secure.value, value);
          expect(secure.writes, 0);
          await bridge.close();
        }
        final secure = MemorySecureStorage()..failRead = true;
        final bridge = open(secure);
        await expectLater(
          bridge.request({'op': 'start', 'online': false}),
          throwsStateError,
        );
        expect(secure.writes, 0);
        await bridge.close();
      });
    },
  );
  test('storage compare-exchange rechecks serialized writes and preserves unknown outcomes', () async {
    final secure = MemorySecureStorage()..value = 'old';
    final store = NativeHubStore(secure: secure);
    expect(
      await Future.wait([
        store.commitIdentityDocument('old', 'first'),
        store.commitIdentityDocument('old', 'stale'),
      ]),
      ['committed', 'conflict'],
    );
    expect(secure.value, 'first');
    secure.failAfterWrite = true;
    expect(await store.commitIdentityDocument('first', 'second'), 'unknown');
    expect(secure.value, 'second');
    final release = await store.acquireIdentityLease();
    await expectLater(
      NativeHubStore().acquireIdentityLease(),
      throwsStateError,
    );
    await release();
    await (await store.acquireIdentityLease())();
  });
}
