import 'dart:async';
import 'dart:typed_data';

import '../models.dart';
import 'bridge.dart';
import 'client_api.dart';
import 'protocol.dart';
import 'store.dart';

/// One downstream session owns every lease and renderer callback.
class RemoteClient extends GatewayClient {
  RemoteClient(
    this.store,
    this.source,
    this.onMessage,
    this.onRetract,
    this.onEvent, {
    this.invitation,
  });
  String? invitation;
  final NativeHubStore store;
  final Json source;
  final void Function(Json) onMessage;
  final void Function(String) onRetract, onEvent;
  SharedHubBridge? _bridge;
  Future<void> Function()? _release;
  Timer? _heartbeat, _retry;
  int _epoch = 0;
  bool _disposed = false;
  bool _closed = false, _suspended = false, _recovering = false;
  final Map<String, _RemoteSurface> _surfaces = {};
  Json? welcome;
  @override
  String? identity;
  @override
  bool online = true;
  @override
  List<GatewayPeer> peers = [];
  @override
  List<HubService> services = [];
  @override
  List<Json> connections = [];
  @override
  bool get usesScopedSurfaces => true;
  @override
  GatewaySurface? surfaceFor(String id) => _surfaces[id];
  Future<dynamic> command(Json value) =>
      _bridge!.request({'op': 'remote', 'request': value});
  Future<Json> rpc(Json value) async =>
      await command({'method': 'rpc', 'value': value}) as Json;
  Future<dynamic> control(Json value) async =>
      (await rpc({'op': 'control', 'command': value}))['value'];
  bool get owner => source['mode'] == 'owner';
  bool get interact => owner || welcome?['permissions']?['interact'] == true;
  bool get editLayout =>
      owner || welcome?['permissions']?['editLayout'] == true;
  @override
  Future<void> start() async {
    _release = await store.acquireIdentityLease();
    if (_closed) {
      await _release?.call();
      _release = null;
      throw StateError('Connection stopped during startup');
    }
    _bridge = SharedHubBridge(store);
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
    welcome = await command(
      invitation == null
          ? {'method': 'connect', 'id': source['id']}
          : {'method': 'register', 'invitation': invitation, 'save': false},
    ) as Json;
    if (_closed || _suspended || epoch != _epoch) return;
    _services(welcome!['services']);
    connections = [
      {'id': source['hubId'], 'name': source['mode'], 'status': 'Connected'},
    ];
    notifyListeners();
    _heartbeat = Timer.periodic(const Duration(seconds: 10), (_) async {
      try {
        final result = await rpc({'op': 'ping'});
        if (epoch == _epoch) _services(result['services']);
      } catch (_) {
        if (epoch == _epoch) unawaited(_lost());
      }
    });
    unawaited(_render(epoch));
  }

  void _services(dynamic value) {
    if (value is List) {
      services = value
          .map((s) => HubService.fromJson(Map<String, dynamic>.from(s as Map)))
          .toList();
    }
    notifyListeners();
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

  Future<void> _lost() async {
    if (_closed || _suspended || _recovering) return;
    _recovering = true;
    _clear();
    connections = [
      {'id': source['hubId'], 'status': 'Disconnected'},
    ];
    notifyListeners();
    try {
      await command({'method': 'disconnect'});
    } finally {
      _recovering = false;
    }
    if (!_closed && !_suspended) {
      _retry = Timer(const Duration(seconds: 3), () => unawaited(_reconnect()));
    }
  }

  Future<void> _reconnect() async {
    if (_closed || _suspended) return;
    try {
      await _connect();
    } catch (e) {
      onEvent('$e');
      if ('$e'.contains('NotAuthorized')) {
        _clear();
        connections = [
          {'id': source['hubId'], 'status': 'Disconnected'},
        ];
        notifyListeners();
        await command({'method': 'disconnect'});
      } else {
        await _lost();
      }
    }
  }

  Future<void> _render(int epoch) async {
    String? ack;
    try {
      while (!_closed && !_suspended && epoch == _epoch) {
        final frame = await command({'method': 'next', 'ack': ?ack}) as Json;
        if (epoch != _epoch) return;
        if (frame['kind'] == 'message') {
          final message = Map<String, dynamic>.from(frame['message'] as Map);
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
            _surfaces[id] = _RemoteSurface(
              this,
              id,
              frame['lease'] as String,
              epoch,
            );
          }
          onMessage(message);
          if (!owner) {
            final routes = _surfaces.keys.map(channelOf).toSet();
            services = routes
                .map(
                  (id) => HubService.fromJson({
                    'id': id,
                    'name': id,
                    'status': 'connected',
                  }),
                )
                .toList();
            notifyListeners();
          }
        }
        ack = frame['sequence'] as String;
        await Future<void>.delayed(Duration.zero);
      }
    } catch (e) {
      if (epoch == _epoch && !_closed && !_suspended) {
        onEvent('$e');
        if ([
          'CapacityExceeded',
          'ProtocolError',
          'NotAuthorized',
        ].any((code) => '$e'.contains(code))) {
          _clear();
          connections = [
            {'id': source['hubId'], 'status': 'Disconnected'},
          ];
          notifyListeners();
          await command({'method': 'disconnect'});
        } else {
          await _lost();
        }
      }
    }
  }

  @override
  Future<void> suspend() async {
    _suspended = true;
    _clear();
    await command({'method': 'disconnect'});
  }

  @override
  Future<void> resume() async {
    if (!_suspended) return;
    _suspended = false;
    await _reconnect();
  }

  @override
  void retry(String id) {
    unawaited(_lost());
  }

  @override
  Future<void> pair(String invitation) async =>
      throw StateError('Use Connections to add a gateway');
  @override
  Future<void> forget(String id) async =>
      throw StateError('Use Connections to forget a Hub');
  @override
  Future<void> setOnline(bool value) async =>
      throw StateError('Remote Hub requires Internet relays');
  @override
  Future<void> send(Json message) async =>
      throw StateError('Surface lease required');
  @override
  Future<Uint8List> resource(String route, String id) async =>
      throw StateError('Surface lease required');
  @override
  Future<void> close() async {
    if (_disposed) return;
    _closed = true;
    _clear();
    await _bridge?.close();
    await _release?.call();
    _release = null;
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
      throw StateError('Surface handle expired');
    }
  }

  @override
  Future<void> send(Json message) async {
    check();
    if (!client.interact) throw StateError('NotAuthorized');
    await client.rpc({'op': 'action', 'lease': lease, 'message': message});
  }

  @override
  GatewayImageRequest image(String uri) {
    check();
    bool cancelled = false;
    final call = client._bridge!.call({
      'op': 'remote',
      'request': {
        'method': 'rpc',
        'value': {'op': 'image', 'lease': lease, 'uri': uri},
      },
    });
    final bytes = call.result.then((value) {
      check();
      if (cancelled) throw StateError('Cancelled');
      return client._bridge!.imageBytes(
        value as Json,
        () => cancelled || client._closed || epoch != client._epoch,
      );
    });
    return GatewayImageRequest(bytes, () {
      cancelled = true;
      call.cancel();
    });
  }
}
