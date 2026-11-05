import 'dart:async';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/models.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/native/client_api.dart';
import 'package:omochi_mobile/src/native/client_remote.dart';
import 'package:omochi_mobile/src/native/store.dart';
import 'package:omochi_mobile/src/remote/controller.dart';
import 'package:omochi_mobile/src/remote/models.dart';
import 'package:omochi_mobile/src/remote/repository.dart';

const source = RemoteSource(
  id: 'hub/grant',
  hubId: 'hub',
  grantId: 'grant',
  mode: RemoteAccessMode.owner,
);

class Transport implements RemoteTransport {
  int connects = 0, closes = 0, disconnects = 0;
  bool failClose = false;
  Completer<void>? finishClose, finishDisconnect;
  final pending = <Completer<RemoteFrame>>[];
  final acks = <String?>[];
  @override
  Future<RemoteWelcome> connect(
    RemoteSource source, {
    String? invitation,
  }) async {
    connects++;
    return const RemoteWelcome();
  }

  @override
  Future<RemoteFrame> next({String? ack}) {
    acks.add(ack);
    final result = Completer<RemoteFrame>();
    pending.add(result);
    return result.future;
  }

  void fail(Object error) => pending.removeAt(0).completeError(error);
  void frame() => pending
      .removeAt(0)
      .complete(
        RemoteFrame.fromJson({
          'kind': 'message',
          'sequence': '1',
          'lease': 'lease',
          'message': {
            'version': 'v0.9',
            'createSurface': {'surfaceId': 'service/main'},
          },
        }),
      );
  void cancel() {
    for (final result in pending) {
      result.completeError(BridgeException('Cancelled', 'cancelled'));
    }
    pending.clear();
  }

  @override
  Future<void> disconnect() async {
    disconnects++;
    cancel();
    await finishDisconnect?.future;
  }

  @override
  Future<void> close() async {
    closes++;
    if (failClose) throw StateError('shutdown failed');
    await finishClose?.future;
    cancel();
  }

  @override
  Future<Json> rpc(Json value) async => {'kind': 'result', 'value': null};
  @override
  GatewayImageRequest image(
    String lease,
    String uri,
    bool Function() expired,
  ) => throw UnimplementedError();
}

class Repository implements RemoteRepository {
  Repository(this.transport);
  final Transport transport;
  Completer<RemoteTransport>? opening;
  final operations = <String>[];
  Completer<List<RemoteSource>>? listing;
  @override
  Future<RemoteTransport> open() => opening?.future ?? Future.value(transport);
  @override
  Future<List<RemoteSource>> list() {
    operations.add('list');
    return listing?.future ?? Future.value([source]);
  }

  @override
  Future<RemoteSource> parse(String invitation) async {
    operations.add('parse');
    return source;
  }

  @override
  Future<void> register(String invitation, {bool save = true}) async {}
  @override
  Future<void> forget(String id) async {}
  @override
  Future<String?> selected() async => source.id;
  @override
  Future<void> select(String? id) async {}
}

class LeaseStore extends NativeHubStore {
  bool held = false;
  final operations = <String>[];
  @override
  Future<Future<void> Function()> acquireIdentityLease() async {
    if (held) throw StateError('Identity already held');
    held = true;
    operations.add('acquire');
    return () async {
      held = false;
      operations.add('release');
    };
  }
}

class CatalogBridge implements SharedHubBridge {
  bool failClose = false;
  int closes = 0;
  @override
  Future<dynamic> request(Json command) async => <Json>[];
  @override
  Future<void> close() async {
    closes++;
    if (failClose) throw StateError('native shutdown failed');
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

RemoteClient client(Repository repository, {void Function(Json)? onMessage}) =>
    RemoteClient(
      NativeHubStore(),
      source,
      onMessage ?? (_) {},
      (_) {},
      (_) {},
      repository: repository,
    );

void main() {
  testWidgets('close awaits pending acquisition and identity shutdown', (
    tester,
  ) async {
    final transport = Transport()..finishClose = Completer<void>();
    final repository = Repository(transport)
      ..opening = Completer<RemoteTransport>();
    final remote = client(repository);
    final started = expectLater(remote.start(), throwsStateError);
    var closed = false;
    final closing = remote.close().then((_) => closed = true);
    await tester.pump();
    expect(closed, isFalse);
    repository.opening!.complete(transport);
    await tester.pump();
    expect(closed, isFalse);
    expect(transport.closes, greaterThanOrEqualTo(1));
    transport.finishClose!.complete();
    await closing;
    await started;
    expect(closed, isTrue);
  });
  testWidgets('failed shutdown retains transport for an awaited retry', (
    tester,
  ) async {
    final transport = Transport();
    final remote = client(Repository(transport));
    await remote.start();
    transport.failClose = true;
    await expectLater(remote.close(), throwsStateError);
    expect(transport.closes, 1);
    transport.failClose = false;
    await remote.close();
    expect(transport.closes, 2);
    await tester.pump();
  });
  testWidgets('suspend and resume serialize disconnect before reconnect', (
    tester,
  ) async {
    final transport = Transport()..finishDisconnect = Completer<void>();
    final remote = client(Repository(transport));
    await remote.start();
    final suspended = remote.suspend();
    final resumed = remote.resume();
    await tester.pump();
    expect(transport.connects, 1);
    transport.finishDisconnect!.complete();
    await suspended;
    await resumed;
    expect(transport.connects, 2);
    expect(remote.state, RemoteConnectionState.connected);
    await remote.close();
    await tester.pump();
  });
  testWidgets('renderer failure never acknowledges or retries its frame', (
    tester,
  ) async {
    final transport = Transport();
    final remote = client(
      Repository(transport),
      onMessage: (_) => throw StateError('cannot apply'),
    );
    await remote.start();
    transport.frame();
    await tester.pump();
    expect(remote.state, RemoteConnectionState.failed);
    expect(transport.acks, [null]);
    await tester.pump(const Duration(seconds: 4));
    expect(transport.connects, 1);
    await remote.close();
  });
  testWidgets(
    'retry policy uses structured code instead of diagnostic substrings',
    (tester) async {
      final transport = Transport();
      final remote = client(Repository(transport));
      await remote.start();
      transport.fail(
        BridgeException(
          'NetworkUnavailable',
          'NotAuthorized appears in diagnostics',
        ),
      );
      await tester.pump();
      expect(remote.state, RemoteConnectionState.disconnected);
      await tester.pump(const Duration(seconds: 3));
      expect(transport.connects, 2);
      transport.fail(
        BridgeException('NotAuthorized', 'temporary network failure'),
      );
      await tester.pump();
      expect(remote.state, RemoteConnectionState.failed);
      await tester.pump(const Duration(seconds: 4));
      expect(transport.connects, 2);
      await remote.close();
    },
  );
  testWidgets('captured renderer handles expire on suspension', (tester) async {
    final transport = Transport();
    final remote = client(Repository(transport));
    await remote.start();
    transport.frame();
    await tester.pump();
    final handle = remote.surfaceFor('service/main')!;
    final suspended = remote.suspend();
    await expectLater(handle.send({}), throwsA(isA<BridgeException>()));
    await suspended;
    await remote.close();
    await tester.pump(Duration.zero);
  });
  testWidgets('native retry policy is authoritative for new failure codes', (
    tester,
  ) async {
    final transport = Transport();
    final remote = client(Repository(transport));
    await remote.start();
    transport.fail(
      BridgeException('FutureFatalCode', 'diagnostic', retryable: false),
    );
    await tester.pump();
    expect(remote.state, RemoteConnectionState.failed);
    await tester.pump(const Duration(seconds: 4));
    expect(transport.connects, 1);
    await remote.close();
  });
  test('catalog operations serialize their repository calls', () async {
    final repository = Repository(Transport())
      ..listing = Completer<List<RemoteSource>>();
    final controller = RemoteConnectionsController(repository);
    final load = controller.load();
    final parse = controller.parse('link');
    await Future<void>.delayed(Duration.zero);
    expect(repository.operations, ['list']);
    repository.listing!.complete([source]);
    await load;
    expect(await parse, same(source));
    expect(repository.operations, ['list', 'parse']);
    controller.dispose();
  });
  test('catalog retries a failed native shutdown before acquiring another identity', () async {
    final store = LeaseStore();
    final first = CatalogBridge()..failClose = true;
    final second = CatalogBridge();
    var opened = 0;
    final repository = NativeRemoteRepository(
      store,
      openBridge: (_) => opened++ == 0 ? first : second,
    );
    await expectLater(repository.list(), throwsStateError);
    expect(store.held, isTrue);
    expect(opened, 1);
    first.failClose = false;
    expect(await repository.list(), isEmpty);
    expect(first.closes, 2);
    expect(store.operations, ['acquire', 'release', 'acquire', 'release']);
    expect(store.held, isFalse);
  });
  test('catalog cannot replace an active renderer transport', () async {
    final store = LeaseStore();
    final bridge = CatalogBridge();
    final repository = NativeRemoteRepository(store, openBridge: (_) => bridge);
    final transport = await repository.open();
    await expectLater(
      repository.open(),
      throwsA(
        isA<BridgeException>().having((e) => e.code, 'code', 'IdentityInUse'),
      ),
    );
    expect(bridge.closes, 0);
    expect(store.held, isTrue);
    await transport.close();
    expect(store.held, isFalse);
  });
}
