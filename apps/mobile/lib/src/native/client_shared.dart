import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';

import '../models.dart';
import 'bridge.dart';
import 'client_api.dart';
import 'protocol.dart';
import 'store.dart';

GatewayClient createSharedGatewayClient({
  required NativeHubStore store,
  required void Function(Json) onMessage,
  required void Function(String) onRetract,
  required void Function(String) onEvent,
  SharedHubBridge Function(NativeHubStore) openBridge = SharedHubBridge.new,
}) => _SharedClient(store, onMessage, onRetract, onEvent, openBridge);

class _SharedClient extends GatewayClient {
  static int _nextClient = 0;
  final int clientId = ++_nextClient;
  Future<void>? _starting;
  _SharedClient(
    this.store,
    this.onMessage,
    this.onRetract,
    this.onEvent,
    this.openBridge,
  );
  final NativeHubStore store;
  final SharedHubBridge Function(NativeHubStore) openBridge;
  final void Function(Json) onMessage;
  final void Function(String) onRetract, onEvent;
  SharedHubBridge? _bridge;
  void Function()? _releaseLease;
  bool _closing = false, _suspended = false;
  bool _foreground = true;
  Future<void> _transitions = Future<void>.value();

  Future<void> _transition(Future<void> Function() operation) {
    final result = _transitions.then((_) async {
      await _starting;
      if (!_closing) await operation();
    });
    _transitions = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return result;
  }

  bool _multicast = false;
  Future<void> _setMulticast(bool enabled) async {
    if (!Platform.isAndroid || enabled == _multicast) return;
    await const MethodChannel('omochi/network')
        .invokeMethod<void>(enabled ? 'acquireMulticast' : 'releaseMulticast');
    _multicast = enabled;
  }

  Future<void>? _closeFuture;
  Timer? _stateTimer;
  int _renderEpoch = 0;
  BridgeCall? _next;
  final Map<String, _Surface> _surfaces = {};
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
  SharedHubBridge get bridge =>
      _bridge ?? (throw StateError('Hub has not started'));

  @override
  Future<void> start() => _starting ??= _start();
  Future<void> _start() async {
    _releaseLease ??= store.acquireIdentityLease();
    try {
      await _setMulticast(true);
      if (_closing) {
        await _setMulticast(false);
        throw StateError('Hub stopped during startup');
      }
      _bridge = openBridge(store);
      online = await store.online();
      await bridge.request({'op': 'start', 'online': online});
      await _refresh();
      await _subscribe();
      _pollState();
    } catch (_) {
      await close();
      rethrow;
    }
  }

  Future<void> _refresh() async {
    final value = await bridge.request({'op': 'state'}) as Json;
    if (_closing) return;
    identity = value['identity'] as String;
    peers = (value['peers'] as List)
        .map((p) => GatewayPeer.fromJson(p))
        .toList();
    services = (value['services'] as List)
        .map((s) => HubService.fromJson(s))
        .toList();
    connections = (value['connections'] as List).cast<Json>().map((c) {
      final status = c['status'] as String;
      return {
        ...c,
        'id': c['nodeId'],
        'name': c['name'] ?? 'Hub',
        'status': status.isEmpty
            ? status
            : '${status[0].toUpperCase()}${status.substring(1)}',
      };
    }).toList();
    notifyListeners();
  }

  void _pollState() {
    _stateTimer?.cancel();
    if (_closing) return;
    _stateTimer = Timer(const Duration(seconds: 1), () async {
      try {
        await _refresh();
      } catch (e) {
        if (!_closing) onEvent('$e');
      }
      _pollState();
    });
  }

  void _discardSurfaces() {
    for (final surface in _surfaces.values) {
      surface.valid = false;
    }
    final routes = _surfaces.keys.map(channelOf).toSet();
    _surfaces.clear();
    for (final route in routes) {
      onRetract(route);
    }
  }

  void _stopRendering() {
    _renderEpoch++;
    _next?.cancel();
    _next = null;
    _discardSurfaces();
  }

  Future<void> _subscribe() async {
    _stopRendering();
    final epoch = _renderEpoch;
    await bridge.request({'op': 'subscribe'});
    if (epoch != _renderEpoch || _closing || _suspended) return;
    unawaited(_render(epoch));
  }

  Future<void> _render(int epoch) async {
    int? ack;
    String? previousFailure;
    while (epoch == _renderEpoch && !_closing && !_suspended) {
      try {
        final request = bridge.call({'op': 'next', 'ack': ?ack});
        _next = request;
        final value = await request.result as Json;
        if (epoch != _renderEpoch || _closing || _suspended) return;
        _next = null;
        final message = value['message'] as Json?;
        if (message != null) {
          final body = [
            'createSurface',
            'updateComponents',
            'updateDataModel',
            'deleteSurface',
          ].map((key) => message[key]).whereType<Map>().firstOrNull;
          final id = body?['surfaceId'] as String?;
          final handle = value['handle'] as int?;
          if (id != null &&
              handle != null &&
              !message.containsKey('deleteSurface')) {
            final old = _surfaces[id];
            if (old?.handle != handle) {
              if (old != null) old.valid = false;
              _surfaces[id] = _Surface(this, id, handle, epoch);
            }
          }
          onMessage(
            message,
          ); // ACK only after synchronous application finishes.
          if (message.containsKey('deleteSurface')) {
            _surfaces.remove(id)?.valid = false;
          }
        }
        ack = value['sequence'] as int;
        // Yield before requesting the next bounded batch so Flutter can build.
        await Future<void>.delayed(Duration.zero);
      } catch (e) {
        if (epoch != _renderEpoch || _closing || _suspended) return;
        _discardSurfaces();
        final failure = '$e';
        onEvent(failure);
        // One resnapshot per distinct failure. A deterministic capacity failure
        // must stop instead of rebuilding the same oversized snapshot forever.
        if (previousFailure == failure || failure.contains('capacity')) return;
        previousFailure = failure;
        ack = null;
        try {
          await bridge.request({'op': 'subscribe'});
        } catch (e) {
          onEvent('$e');
          return;
        }
      }
    }
  }

  Future<void> _mutation(Json command) async {
    try {
      await bridge.request(command);
    } finally {
      // A failed local save can still leave a registered peer or suspend auth.
      // Refresh the core's state; never reconstruct it from the invitation.
      if (!_closing) {
        try {
          await _refresh();
        } catch (_) {}
      }
    }
  }

  @override
  Future<void> pair(String invitation) =>
      _mutation({'op': 'pair', 'invitation': invitation});
  @override
  Future<void> forget(String id) {
    _retractGateway(id);
    return _mutation({'op': 'forget', 'id': id});
  }

  void _retractGateway(String id) {
    final affected = _surfaces.keys
        .where((surface) => surface.startsWith('$id~'))
        .toList();
    for (final surface in affected) {
      _surfaces.remove(surface)?.valid = false;
    }
    for (final route in affected.map(channelOf).toSet()) {
      onRetract(route);
    }
  }

  @override
  void retry(String id) {
    _retractGateway(id);
    unawaited(
      _mutation({'op': 'retry', 'id': id})
          .catchError((Object e) => onEvent('$e')),
    );
  }

  @override
  Future<void> setOnline(bool value) async {
    await store.setOnline(value);
    await _transition(() async {
      online = value;
      if (_foreground) {
        await _resume();
      } else {
        notifyListeners();
      }
    });
  }

  @override
  Future<void> suspend() {
    _foreground = false;
    return _transition(_suspend);
  }

  Future<void> _suspend() async {
    _suspended = true;
    _stopRendering();
    await bridge.request({'op': 'suspend'});
    await _setMulticast(false);
    await _refresh();
  }

  @override
  Future<void> resume() {
    _foreground = true;
    return _transition(() async {
      if (_foreground) await _resume();
    });
  }

  Future<void> _resume() async {
    // A resumed notification can arrive without a preceding paused event.
    // Invalidate the old subscription before Rust replaces its endpoint.
    _suspended = true;
    _stopRendering();
    await _setMulticast(true);
    if (_closing) {
      await _setMulticast(false);
      throw StateError('Hub stopped during resume');
    }
    if (!_foreground) {
      await _setMulticast(false);
      return;
    }
    await bridge.request({'op': 'resume', 'online': online});
    if (_closing) return;
    if (!_foreground) {
      await _suspend();
      return;
    }
    _suspended = false;
    await _subscribe();
    await _refresh();
  }

  @override
  Future<void> send(Json message) async =>
      throw StateError('A captured Surface handle is required');
  @override
  Future<Uint8List> resource(String route, String id) async =>
      throw StateError('A captured Surface handle is required');
  @override
  Future<void> close() => _closeFuture ??= _close().catchError((Object e) {
    _closeFuture = null;
    throw e;
  });
  Future<void> _close() async {
    _closing = true;
    _stateTimer?.cancel();
    _stopRendering();
    await _bridge?.close();
    _bridge = null;
    await _setMulticast(false);
    _releaseLease?.call();
    _releaseLease = null;
    super.dispose();
  }
}

class _Surface implements GatewaySurface {
  _Surface(this.owner, this.surfaceId, this.handle, this.epoch);
  final _SharedClient owner;
  @override
  final String surfaceId;
  final int handle, epoch;
  bool valid = true;
  @override
  String get renderId => 'native:${owner.clientId}:$epoch:$handle';
  void check() {
    if (!valid || owner._closing || owner._renderEpoch != epoch) {
      throw StateError('Surface handle expired');
    }
  }

  @override
  Future<void> send(Json message) async {
    check();
    await owner.bridge.request({
      'op': 'action',
      'handle': handle,
      'message': message,
    });
  }

  @override
  GatewayImageRequest image(String uri) {
    check();
    var cancelled = false;
    final request = owner.bridge.call({
      'op': 'image',
      'handle': handle,
      'uri': uri,
    });
    final result = request.result.then(
      (value) => owner.bridge.imageBytes(
        value as Json,
        () =>
            cancelled ||
            !valid ||
            owner._renderEpoch != epoch ||
            owner._closing,
      ),
    );
    return GatewayImageRequest(result, () {
      cancelled = true;
      request.cancel();
    });
  }
}
