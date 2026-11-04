import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/hub_session.dart';

import 'scoped_renderer_test.dart' show TestClient, TestStore;

class ShutdownClient extends TestClient {
  int closeAttempts = 0;
  bool closed = false;
  Future<void> Function(int attempt)? shutdown;

  @override
  Future<void> close() {
    closeAttempts++;
    final stopping = shutdown?.call(closeAttempts) ?? Future<void>.value();
    return stopping.then((_) {
      closed = true;
    });
  }
}

class ShutdownFixture {
  final first = ShutdownClient();
  final replacement = ShutdownClient();
  int created = 0;
  late final hub = HubSession(
    nativeStore: TestStore(),
    gatewayClientFactory:
        ({
          required store,
          required onMessage,
          required onRetract,
          required onEvent,
        }) {
          if (created > 0) {
            expect(first.closed, isTrue, reason: 'old identity lease is held');
          }
          final client = created++ == 0 ? first : replacement;
          client.onMessage = onMessage;
          client.onRetract = onRetract;
          return client;
        },
  );
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test(
    'retry closes a failed client before creating its replacement',
    () async {
      final fixture = ShutdownFixture();
      final retryClose = Completer<void>();
      fixture.first.shutdown = (attempt) async {
        if (attempt == 1) throw StateError('temporary shutdown failure');
        await retryClose.future;
      };
      final hub = fixture.hub;
      await hub.startNative();
      await expectLater(hub.startNative(), throwsStateError);
      expect(hub.status, 'Unavailable');
      expect(fixture.created, 1);

      final retry = hub.startNative();
      await Future<void>.delayed(Duration.zero);
      expect(fixture.first.closeAttempts, 2);
      expect(fixture.created, 1);
      expect(hub.status, 'Starting');
      // The old client cannot publish state into the new epoch while closing.
      fixture.first.notifyListeners();
      expect(hub.status, 'Starting');
      retryClose.complete();
      await retry;
      expect(fixture.created, 2);
      expect(hub.status, 'Connected');
      await hub.stopNative();
      hub.dispose();
    },
  );

  test(
    'concurrent retries share close and only the latest start wins',
    () async {
      final fixture = ShutdownFixture();
      final retryClose = Completer<void>();
      fixture.first.shutdown = (attempt) async {
        if (attempt == 1) throw StateError('temporary shutdown failure');
        await retryClose.future;
      };
      final hub = fixture.hub;
      await hub.startNative();
      await expectLater(hub.startNative(), throwsStateError);
      final earlier = hub.startNative();
      final latest = hub.startNative();
      await Future<void>.delayed(Duration.zero);
      expect(fixture.first.closeAttempts, 2);
      expect(fixture.created, 1);
      retryClose.complete();
      await Future.wait([earlier, latest]);
      expect(fixture.created, 2);
      expect(hub.status, 'Connected');
      await hub.stopNative();
      hub.dispose();
    },
  );

  test('a synchronous close failure also leaves shutdown retryable', () async {
    final fixture = ShutdownFixture();
    fixture.first.shutdown = (attempt) {
      if (attempt == 1) throw StateError('synchronous shutdown failure');
      return Future<void>.value();
    };
    final hub = fixture.hub;
    await hub.startNative();
    await expectLater(hub.startNative(), throwsStateError);
    expect(fixture.created, 1);
    await hub.startNative();
    expect(fixture.first.closeAttempts, 2);
    expect(fixture.created, 2);
    await hub.stopNative();
    hub.dispose();
  });

  test('stop supersedes a pending retry without creating a client', () async {
    final fixture = ShutdownFixture();
    final close = Completer<void>();
    fixture.first.shutdown = (_) => close.future;
    final hub = fixture.hub;
    await hub.startNative();
    final start = hub.startNative();
    final stop = hub.stopNative();
    close.complete();
    await Future.wait([start, stop]);
    expect(fixture.first.closeAttempts, 1);
    expect(fixture.created, 1);
    expect(hub.status, 'Stopped');
    hub.dispose();
  });

  test('a pending stop cannot overwrite a newer start', () async {
    final fixture = ShutdownFixture();
    final close = Completer<void>();
    fixture.first.shutdown = (_) => close.future;
    final hub = fixture.hub;
    await hub.startNative();
    final stop = hub.stopNative();
    final start = hub.startNative();
    close.complete();
    await Future.wait([stop, start]);
    expect(fixture.first.closeAttempts, 1);
    expect(fixture.created, 2);
    expect(hub.status, 'Connected');
    await hub.stopNative();
    hub.dispose();
  });

  test(
    'dispose observes close failure while an awaited stop still fails',
    () async {
      final fixture = ShutdownFixture();
      final close = Completer<void>();
      fixture.first.shutdown = (_) => close.future;
      final hub = fixture.hub;
      await hub.startNative();
      final stopped = expectLater(hub.stopNative(), throwsStateError);
      hub.dispose();
      close.completeError(StateError('temporary shutdown failure'));
      await stopped;
      await Future<void>.delayed(Duration.zero);
      expect(hub.error, contains('temporary shutdown failure'));
      expect(fixture.first.closeAttempts, 1);
      expect(fixture.created, 1);
      // Even after disposal, an explicit stop can finish the retained shutdown.
      fixture.first.shutdown = null;
      await hub.stopNative();
      expect(fixture.first.closeAttempts, 2);
      expect(fixture.first.closed, isTrue);
    },
  );

  test('dispose handles its own asynchronous shutdown failure', () async {
    final fixture = ShutdownFixture();
    final close = Completer<void>();
    fixture.first.shutdown = (_) => close.future;
    final hub = fixture.hub;
    await hub.startNative();
    hub.dispose();
    close.completeError(StateError('temporary shutdown failure'));
    await Future<void>.delayed(Duration.zero);
    expect(hub.error, contains('temporary shutdown failure'));
    expect(hub.log.first.direction, 'ERROR');
    expect(fixture.created, 1);
    fixture.first.shutdown = null;
    await hub.stopNative();
    expect(fixture.first.closed, isTrue);
    await expectLater(hub.startNative(), throwsStateError);
    expect(fixture.created, 1);
  });

  test('dispose does not hide failure from an awaiting restart', () async {
    final fixture = ShutdownFixture();
    final close = Completer<void>();
    fixture.first.shutdown = (_) => close.future;
    final hub = fixture.hub;
    await hub.startNative();
    final restarted = expectLater(hub.startNative(), throwsStateError);
    hub.dispose();
    close.completeError(StateError('temporary shutdown failure'));
    await restarted;
    expect(fixture.created, 1);
    fixture.first.shutdown = null;
    await hub.stopNative();
    expect(fixture.first.closed, isTrue);
  });
}
