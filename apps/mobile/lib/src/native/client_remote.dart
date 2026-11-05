import 'dart:async';

import '../models.dart';
import '../remote/board_models.dart';
import '../remote/models.dart';
import '../remote/repository.dart';
import 'bridge.dart';
import 'client_api.dart';
import 'protocol.dart';
import 'store.dart';

/// Connection controller. Native transport owns the bridge and identity lease;
/// this controller owns Flutter scheduling and captured renderer handles.
class RemoteClient extends HubClient {
  RemoteClient(
    NativeHubStore store,
    this.source,
    this.onMessage,
    this.onRetract,
    this.onEvent, {
    this.invitation,
    RemoteRepository? repository,
  }) : repository = repository ?? NativeRemoteRepository(store);
  final RemoteRepository repository;
  final RemoteSource source;
  final String? invitation;
  final void Function(Json) onMessage;
  final void Function(String) onRetract, onEvent;
  RemoteTransport? _transport;
  Future<RemoteTransport>? _openingTransport;
  Future<void>? _starting, _closing;
  Future<void> _transitions = Future.value();
  Timer? _heartbeat, _retry;
  int _epoch = 0;
  bool _disposed = false, _closed = false, _suspended = false;
  final Map<String, _RemoteSurface> _surfaces = {};
  RemoteWelcome? welcome;
  RemoteConnectionState state = RemoteConnectionState.disconnected;
  @override
  String? get identity => null;
  @override
  bool get online => true;
  @override
  List<GatewayPeer> get peers => const [];
  @override
  List<HubService> services = [];
  @override
  List<Json> get connections => [
    {
      'id': source.hubId,
      'name': source.mode.name,
      'status': state == RemoteConnectionState.connected
          ? 'Connected'
          : 'Disconnected',
    },
  ];
  @override
  bool get usesScopedSurfaces => true;
  @override
  GatewaySurface? surfaceFor(String id) => _surfaces[id];
  bool get owner => source.mode == RemoteAccessMode.owner;
  bool get interact => owner || welcome?.permissions.interact == true;
  bool get editLayout => owner || welcome?.permissions.editLayout == true;
  void updateSharedBoard(RemoteBoard board) {
    final current = welcome;
    if (current == null) return;
    welcome = RemoteWelcome(
      services: current.services,
      permissions: current.permissions,
      board: board,
    );
    _emit();
  }

  /// Only repository adapters use this wire seam. Widgets use typed methods.
  Future<Json> rpc(Json value) => _connection.rpc(value);
  Future<Object?> control(Json value) async =>
      (await rpc({'op': 'control', 'command': value}))['value'];
  RemoteTransport get _connection =>
      _transport ??
      (throw BridgeException('ScopeExpired', 'Connection has not started'));
  void _emit() {
    if (!_closed && !_disposed) notifyListeners();
  }

  Future<void> _transition(Future<void> Function() operation) {
    final result = _transitions.then((_) async {
      await _starting;
      if (!_closed) await operation();
    });
    _transitions = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return result;
  }

  @override
  Future<void> start() => _starting ??= _start();
  Future<void> _start() async {
    if (_closed) throw StateError('Connection has stopped');
    final transport = await (_openingTransport = repository.open());
    if (_closed) {
      await transport.close();
      throw StateError('Connection stopped during startup');
    }
    _transport = transport;
    try {
      await _connect();
    } catch (_) {
      await close();
      rethrow;
    }
  }

  Future<void> _connect() async {
    _clear();
    final epoch = _epoch;
    state = RemoteConnectionState.connecting;
    _emit();
    final value = await _connection.connect(source, invitation: invitation);
    if (_closed || _suspended || epoch != _epoch) return;
    welcome = value;
    services = value.services;
    state = RemoteConnectionState.connected;
    _emit();
    _scheduleHeartbeat(epoch);
    unawaited(_render(epoch));
  }

  void _scheduleHeartbeat(int epoch) {
    if (_closed || _suspended || epoch != _epoch) return;
    _heartbeat = Timer(const Duration(seconds: 10), () async {
      try {
        final result = await rpc({'op': 'ping'});
        if (epoch != _epoch || _closed || _suspended) return;
        if (owner && result['services'] is List) {
          services = (result['services'] as List)
              .map(
                (s) => HubService.fromJson(Map<String, dynamic>.from(s as Map)),
              )
              .toList();
        }
        _emit();
        _scheduleHeartbeat(epoch);
      } catch (e) {
        await _failed(e, epoch);
      }
    });
  }

  void _clear() {
    _epoch++;
    _heartbeat?.cancel();
    _retry?.cancel();
    for (final route in _surfaces.keys.map(channelOf).toSet()) {
      onRetract(route);
    }
    _surfaces.clear();
    services = [];
    welcome = null;
  }

  bool _terminal(Object error) {
    if (error is! BridgeException) return false;
    if (error.retryable case final bool retryable) return !retryable;
    // Older bridge builds omit policy; preserve exact-code compatibility.
    return const {
      'CapacityExceeded',
      'ProtocolError',
      'NotAuthorized',
      'InvalidInvitation',
      'NotFound',
      'StorageCorrupt',
      'StorageUnavailable',
      'StorageOutcomeUnknown',
      'RenderError',
      'BridgeTerminated',
    }.contains(error.code);
  }

  Future<void> _failed(Object error, int epoch) async {
    if (_closed || _suspended || epoch != _epoch) return;
    final terminal = _terminal(error);
    _clear();
    final failureEpoch = _epoch;
    state = terminal
        ? RemoteConnectionState.failed
        : RemoteConnectionState.disconnected;
    onEvent('$error');
    _emit();
    await _transition(() async {
      try {
        await _connection.disconnect();
      } catch (e) {
        if (!_closed) onEvent('$e');
      }
      if (!terminal && !_suspended && failureEpoch == _epoch) _scheduleRetry();
    });
  }

  void _scheduleRetry() {
    if (_closed || _suspended) return;
    _retry?.cancel();
    _retry = Timer(const Duration(seconds: 3), () => unawaited(_reconnect()));
  }

  Future<void> _reconnect() => _transition(() async {
    if (_suspended) return;
    try {
      await _connect();
    } catch (e) {
      if (_closed || _suspended) return;
      _clear();
      state = _terminal(e)
          ? RemoteConnectionState.failed
          : RemoteConnectionState.disconnected;
      onEvent('$e');
      _emit();
      try {
        await _connection.disconnect();
      } catch (e) {
        onEvent('$e');
      }
      if (state != RemoteConnectionState.failed) _scheduleRetry();
    }
  });

  Future<void> _render(int epoch) async {
    String? ack;
    try {
      while (!_closed && !_suspended && epoch == _epoch) {
        final frame = await _connection.next(ack: ack);
        if (epoch != _epoch || _closed || _suspended) return;
        if (frame.kind == RemoteFrameKind.message) {
          final message = frame.message!;
          final body = [
            'createSurface',
            'updateComponents',
            'updateDataModel',
            'deleteSurface',
          ].map((k) => message[k]).whereType<Map>().single;
          final id = body['surfaceId'] as String;
          if (message.containsKey('deleteSurface')) {
            _surfaces.remove(id);
          } else {
            _surfaces[id] = _RemoteSurface(this, id, frame.lease!, epoch);
          }
          try {
            onMessage(message); // ACK only after synchronous application.
          } catch (e) {
            throw BridgeException('RenderError', '$e', retryable: false);
          }
          if (!owner) {
            services = _surfaces.keys
                .map(channelOf)
                .toSet()
                .map(
                  (id) => HubService.fromJson({
                    'id': id,
                    'name': id,
                    'status': 'connected',
                  }),
                )
                .toList();
            _emit();
          }
        }
        ack = frame.sequence;
        await Future<void>.delayed(Duration.zero);
      }
    } catch (e) {
      await _failed(e, epoch);
    }
  }

  @override
  Future<void> suspend() {
    _suspended = true;
    _clear();
    state = RemoteConnectionState.suspended;
    _emit();
    return _transition(() => _connection.disconnect());
  }

  @override
  Future<void> resume() => _transition(() async {
    if (!_suspended) return;
    _suspended = false;
    try {
      await _connect();
    } catch (e) {
      _clear();
      state = _terminal(e)
          ? RemoteConnectionState.failed
          : RemoteConnectionState.disconnected;
      onEvent('$e');
      _emit();
      if (state != RemoteConnectionState.failed) _scheduleRetry();
    }
  });
  @override
  void retry(String id) {
    _retry?.cancel();
    unawaited(_reconnect());
  }

  @override
  Future<void> close() => _closing ??= _close().catchError((Object e) {
    _closing = null;
    throw e;
  });
  Future<void> _close() async {
    if (_disposed) return;
    _closed = true;
    _clear();
    state = RemoteConnectionState.closed;
    RemoteTransport? pending;
    try {
      pending = await _openingTransport;
    } catch (_) {
      // A failed acquisition owns no transport. Its startup Future reports it.
    }
    await (_transport ?? pending)?.close();
    _transport = null;
    _disposed = true;
    super.dispose();
  }
}

class _RemoteSurface implements GatewaySurface {
  _RemoteSurface(this.client, this.surfaceId, this.lease, this.epoch);
  final RemoteClient client;
  @override
  final String surfaceId;
  final String lease;
  final int epoch;
  @override
  String get renderId => 'remote:$epoch:$lease';
  void check() {
    if (client._closed ||
        epoch != client._epoch ||
        client._surfaces[surfaceId]?.lease != lease) {
      throw BridgeException('ScopeExpired', 'Surface handle expired');
    }
  }

  @override
  Future<void> send(Json message) async {
    check();
    if (!client.interact) {
      throw BridgeException('NotAuthorized', 'Interaction is not allowed');
    }
    await client.rpc({'op': 'action', 'lease': lease, 'message': message});
  }

  @override
  GatewayImageRequest image(String uri) {
    check();
    return client._connection.image(
      lease,
      uri,
      () =>
          client._closed ||
          epoch != client._epoch ||
          client._surfaces[surfaceId]?.lease != lease,
    );
  }
}
