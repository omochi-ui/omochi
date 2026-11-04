import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/store.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  const channel = MethodChannel('omochi/identity-lease');
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

  tearDown(() {
    debugDefaultTargetPlatformOverride = null;
    messenger.setMockMethodCallHandler(channel, null);
  });

  for (final platform in [TargetPlatform.android, TargetPlatform.macOS]) {
    test(
      '$platform lease failures preserve ownership and allow retry',
      () async {
        debugDefaultTargetPlatformOverride = platform;
        final store = NativeHubStore(namespace: 'omochi.test.lease.$platform');
        final calls = <String>[];
        var rejectAcquire = true;
        var nullAcquire = false;
        var rejectRelease = true;
        messenger.setMockMethodCallHandler(channel, (call) async {
          calls.add(call.method);
          if (call.method == 'acquire') {
            expect(call.arguments, store.namespace);
            if (rejectAcquire) throw PlatformException(code: 'identity-lease');
            return nullAcquire ? null : 'platform-token';
          }
          expect(call.method, 'release');
          expect(call.arguments, 'platform-token');
          if (rejectRelease) throw PlatformException(code: 'identity-lease');
          return null;
        });
        await expectLater(
          store.acquireIdentityLease(),
          throwsA(isA<PlatformException>()),
        );
        rejectAcquire = false;
        nullAcquire = true;
        await expectLater(store.acquireIdentityLease(), throwsStateError);
        nullAcquire = false;
        final release = await store.acquireIdentityLease();
        final other = NativeHubStore(namespace: store.namespace);
        await expectLater(other.acquireIdentityLease(), throwsStateError);
        expect(calls, ['acquire', 'acquire', 'acquire']);
        await expectLater(release(), throwsA(isA<PlatformException>()));
        await expectLater(other.acquireIdentityLease(), throwsStateError);
        rejectRelease = false;
        await release();
        await release();
        expect(calls, ['acquire', 'acquire', 'acquire', 'release', 'release']);
        await (await other.acquireIdentityLease())();
        expect(calls.last, 'release');
      },
    );
  }
}
