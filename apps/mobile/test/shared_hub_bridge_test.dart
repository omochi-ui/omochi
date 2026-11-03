import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/native/store.dart';

import 'support/memory_secure_storage.dart';

void main() {
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
    final release = store.acquireIdentityLease();
    expect(() => NativeHubStore().acquireIdentityLease(), throwsStateError);
    release();
    store.acquireIdentityLease()();
  });
}
