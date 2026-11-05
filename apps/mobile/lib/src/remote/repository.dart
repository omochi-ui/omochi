import 'dart:typed_data';

import 'package:shared_preferences/shared_preferences.dart';

import '../models.dart';
import '../native/bridge.dart';
import '../native/client_api.dart';
import '../native/store.dart';
import 'models.dart';

abstract interface class RemoteRepository {
  Future<List<RemoteSource>> list();
  Future<RemoteSource> parse(String invitation);
  Future<void> register(String invitation, {bool save = true});
  Future<void> forget(String id);
  Future<String?> selected();
  Future<void> select(String? id);
  Future<RemoteTransport> open();
}

/// Owns a single native bridge and its identity lease until successful shutdown.
abstract interface class RemoteTransport {
  Future<RemoteWelcome> connect(RemoteSource source, {String? invitation});
  Future<RemoteFrame> next({String? ack});
  Future<Json> rpc(Json value);
  GatewayImageRequest image(String lease, String uri, bool Function() expired);
  Future<void> disconnect();
  Future<void> close();
}

class NativeRemoteRepository implements RemoteRepository {
  NativeRemoteRepository(this.store, {this.openBridge = SharedHubBridge.new});
  factory NativeRemoteRepository.mobile() => NativeRemoteRepository(
    NativeHubStore(namespace: 'omochi.mobile.remote.v1'),
  );
  final NativeHubStore store;
  final SharedHubBridge Function(NativeHubStore) openBridge;
  static const selectedKey = 'omochi.mobile.selectedHub';
  _NativeRemoteTransport? _retained;
  Future<void> _opening = Future.value();
  @override
  Future<RemoteTransport> open() {
    final result = _opening.then((_) => _open());
    _opening = result.then<void>((_) {}, onError: (Object _, StackTrace _) {});
    return result;
  }

  Future<RemoteTransport> _open() async {
    // A failed shutdown still owns the identity. Retry it before acquiring.
    if (_retained case final transport? when !transport._closeRequested) {
      throw BridgeException(
        'IdentityInUse',
        'A remote connection is already open',
      );
    }
    await _retained?.close();
    _retained = null;
    final release = await store.acquireIdentityLease();
    try {
      return _retained = _NativeRemoteTransport(openBridge(store), release);
    } catch (_) {
      await release();
      rethrow;
    }
  }

  Future<T> _withBridge<T>(
    Future<T> Function(SharedHubBridge) operation,
  ) async {
    final transport = await open() as _NativeRemoteTransport;
    try {
      return await operation(transport.bridge);
    } finally {
      await transport.close();
    }
  }

  @override
  Future<List<RemoteSource>> list() => _withBridge((bridge) async {
    final value = await bridge.request({
      'op': 'remote',
      'request': {'method': 'list'},
    }) as List;
    return value
        .map((v) => RemoteSource.fromJson(Map<String, dynamic>.from(v as Map)))
        .toList();
  });
  @override
  Future<RemoteSource> parse(String invitation) => _withBridge(
    (bridge) async => RemoteSource.fromJson(
      await bridge.request({
        'op': 'remote',
        'request': {'method': 'parse', 'invitation': invitation},
      }) as Json,
    ),
  );
  @override
  Future<void> register(String invitation, {bool save = true}) => _withBridge((
    bridge,
  ) async {
    await bridge.request({
      'op': 'remote',
      'request': {'method': 'register', 'invitation': invitation, 'save': save},
    });
  });
  @override
  Future<void> forget(String id) => _withBridge((bridge) async {
    await bridge.request({
      'op': 'remote',
      'request': {'method': 'forget', 'id': id},
    });
  });
  @override
  Future<String?> selected() => SharedPreferencesAsync().getString(selectedKey);
  @override
  Future<void> select(String? id) async {
    final preferences = SharedPreferencesAsync();
    if (id == null) {
      await preferences.remove(selectedKey);
    } else {
      await preferences.setString(selectedKey, id);
    }
  }
}

class _NativeRemoteTransport implements RemoteTransport {
  _NativeRemoteTransport(this.bridge, this.release);
  final SharedHubBridge bridge;
  final Future<void> Function() release;
  Future<void>? _closing;
  bool _closed = false;
  bool _closeRequested = false;
  Future<Object?> _command(Json value) =>
      bridge.request({'op': 'remote', 'request': value});
  @override
  Future<RemoteWelcome> connect(
    RemoteSource source, {
    String? invitation,
  }) async => RemoteWelcome.fromJson(
    await _command(
      invitation == null
          ? {'method': 'connect', 'id': source.id}
          : {'method': 'register', 'invitation': invitation, 'save': false},
    ) as Json,
  );
  @override
  Future<RemoteFrame> next({String? ack}) async => RemoteFrame.fromJson(
    await _command({'method': 'next', 'ack': ?ack}) as Json,
  );
  @override
  Future<Json> rpc(Json value) async =>
      await _command({'method': 'rpc', 'value': value}) as Json;
  @override
  Future<void> disconnect() async {
    await _command({'method': 'disconnect'});
  }

  @override
  GatewayImageRequest image(String lease, String uri, bool Function() expired) {
    var cancelled = false;
    final call = bridge.call({
      'op': 'remote',
      'request': {
        'method': 'rpc',
        'value': {'op': 'image', 'lease': lease, 'uri': uri},
      },
    });
    final Future<Uint8List> result = call.result.then(
      (value) => bridge.imageBytes(value as Json, () => cancelled || expired()),
    );
    return GatewayImageRequest(result, () {
      cancelled = true;
      call.cancel();
    });
  }

  @override
  Future<void> close() {
    _closeRequested = true;
    return _closing ??= _close().catchError((Object e) {
      _closing = null;
      throw e;
    });
  }

  Future<void> _close() async {
    if (_closed) return;
    await bridge.close();
    await release();
    _closed = true;
  }
}
