import 'messages.dart';

import 'dart:async';
import 'dart:convert';

import 'package:a2ui_core/a2ui_core.dart' as core;
import 'package:flutter/foundation.dart';
import 'package:genui/genui.dart';

import 'context_codec.dart';
import 'models.dart';
import 'log_redaction.dart';
import 'native/client.dart';
import 'native/client_remote.dart';
import 'native/modal.dart';
import 'native/protocol.dart';
import 'native/resource_image.dart';
import 'native/store.dart';
import 'remote/models.dart';

class WireEntry {
  WireEntry(this.direction, this.body) : time = DateTime.now();
  final String direction, body;
  final DateTime time;
}

/// GenUI rendering and OS integration for the shared Rust Hub.
/// A connection epoch owns all async work.
/// No actions are buffered or replayed after a disconnect.
class HubSession extends ChangeNotifier {
  HubSession({
    NativeHubStore? nativeStore,
    this.gatewayClientFactory = createSharedGatewayClient,
  }) : nativeStore = nativeStore ?? NativeHubStore() {
    _newRenderer();
  }
  late SurfaceController renderer;
  late NativeModalRegistry _modals;
  StreamSubscription<ChatMessage>? _submits;
  int _epoch = 0;
  bool _disposed = false;
  final NativeHubStore nativeStore;
  final GatewayClientFactory gatewayClientFactory;
  RemoteClient? get remoteClient =>
      _native is RemoteClient ? _native as RemoteClient : null;
  Future<bool> Function()? remoteLeaveGuard;
  HubClient? _native;
  HubClient? _closingNative;
  Future<void>? _nativeStopped;
  final Map<String, GatewaySurface> _renderScopes = {};
  final Map<String, String> _surfaceRenderIds = {};
  String? get nativeIdentity => _native?.identity;
  bool get nativeOnline => _native?.online ?? true;
  bool get canManageGateways => _native is GatewayClient;
  GatewayClient get _gateway => _native is GatewayClient
      ? _native as GatewayClient
      : throw StateError('Local gateway administration is unavailable');
  List<GatewayPeer> get gateways => _native?.peers ?? [];
  String status = 'Stopped';
  String? error;
  List<HubService> services = [];
  List<dynamic> connections = [];
  final List<WireEntry> log = [];
  final Set<String> _sendData = {};
  final Set<String> _unscopedErrors = {};
  final ContextCodec _contexts = ContextCodec();
  final Map<String, String> surfaceErrors = {};
  int rendererVersion = 0;
  int get epoch => _epoch;
  List<String> get surfaceIds => renderer.activeSurfaceIds
      .map((id) => _renderScopes[id]?.surfaceId ?? id)
      .toList();
  SurfaceContext contextForSurface(String id) =>
      renderer.contextFor(_surfaceRenderIds[id] ?? id);
  bool get connected => status == 'Connected';

  void _emit() {
    if (!_disposed) notifyListeners();
  }

  void record(String direction, Object value) {
    final raw = diagnosticText(value);
    log.insert(
      0,
      WireEntry(
        direction,
        raw.length > 4096 ? '${raw.substring(0, 4096)}…' : raw,
      ),
    );
    if (log.length > 100) log.removeLast();
  }

  void _newRenderer() {
    _modals = NativeModalRegistry();
    renderer = SurfaceController(
      catalogs: [
        nativeCatalog(
          (surfaceId, uri) {
            final scope = _renderScopes[surfaceId];
            if (scope != null) return scope.image(uri);
            throw StateError(messages.errorImageNeedsNativeHub);
          },
          useHub: () => true,
          modals: _modals,
        ),
      ],
    );
    _sendData.clear();
    _unscopedErrors.clear();
    _contexts.clear();
    _renderScopes.clear();
    _surfaceRenderIds.clear();
    surfaceErrors.clear();
    rendererVersion++;
    _submits = renderer.onSubmit.listen((message) {
      for (final part in message.parts.uiInteractionParts) {
        try {
          _submit(jsonDecode(part.interaction) as Json);
        } catch (e) {
          error = messages.errorActionFailed(describeFailure(e));
          _emit();
        }
      }
    });
  }

  void _clearRenderer() {
    _modals.clear();
    _submits?.cancel();
    renderer.dispose();
    _newRenderer();
  }

  void receive(Object value) {
    try {
      if (value is String && utf8.encode(value).length > 4 * 1024 * 1024) {
        throw FormatException(messages.errorOversizedMessage);
      }
      final j = value is String
          ? jsonDecode(value) as Json
          : Map<String, dynamic>.from(value as Map);
      if (j['omochi'] == 'resources/1' && j['kind'] == 'hello') return;
      const keys = [
        'createSurface',
        'updateComponents',
        'updateDataModel',
        'deleteSurface',
      ];
      if (j['version'] != 'v0.9' || keys.where(j.containsKey).length != 1) {
        throw FormatException(messages.errorInvalidServerMessage);
      }
      record('IN', j);
      if (j['createSurface'] case final Map create) {
        final id = create['surfaceId'] as String;
        if (create['sendDataModel'] == true) {
          _sendData.add(id);
        } else {
          _sendData.remove(id);
        }
      }
      if (j['deleteSurface'] case final Map deleted) {
        _sendData.remove(deleted['surfaceId']);
        surfaceErrors.remove(deleted['surfaceId']);
      }
      var renderMessage = j;
      if (_native?.usesScopedSurfaces == true) {
        final key = keys.firstWhere(j.containsKey);
        final body = Map<String, dynamic>.from(j[key] as Map);
        final id = body['surfaceId'] as String;
        final scope = _native!.surfaceFor(id);
        final renderId = key == 'deleteSurface'
            ? _surfaceRenderIds[id]
            : scope?.renderId;
        if (renderId == null) {
          if (key == 'deleteSurface') return;
          throw StateError('Render batch has no Surface handle');
        }
        final old = _surfaceRenderIds[id];
        if (old != null && old != renderId) {
          _modals.remove(old);
          renderer.handleMessage(
            core.A2uiMessage.fromJson(
              _contexts.incoming({
                'version': 'v0.9',
                'deleteSurface': {'surfaceId': old},
              }),
            ),
          );
          _renderScopes.remove(old);
        }
        if (key == 'deleteSurface') {
          _renderScopes.remove(renderId);
          _surfaceRenderIds.remove(id);
        } else {
          _renderScopes[renderId] = scope!;
          _surfaceRenderIds[id] = renderId;
        }
        renderMessage = {
          ...j,
          key: {...body, 'surfaceId': renderId},
        };
      }
      if (renderMessage['createSurface'] case final Map created) {
        _modals.create(created['surfaceId'] as String);
      }
      if (renderMessage['deleteSurface'] case final Map deleted) {
        _modals.remove(deleted['surfaceId'] as String);
      }
      renderer.handleMessage(
        core.A2uiMessage.fromJson(_contexts.incoming(renderMessage)),
      );
    } catch (e) {
      error = messages.errorRenderingFailed(describeFailure(e));
      record('ERROR', error!);
      if (_native?.usesScopedSurfaces == true) rethrow;
    }
    _emit();
  }

  /// Scope model metadata to the target service, like the native Hub boundary.
  Json actionEnvelope(Json action) {
    action = _contexts.outgoing(action);
    if (_native?.usesScopedSurfaces == true) {
      final scope = _renderScopes[action['surfaceId']];
      if (scope == null) throw StateError('Surface handle expired');
      action = {...action, 'surfaceId': scope.surfaceId};
    }
    // GenUI emits local ISO timestamps without an offset; the SDK requires RFC3339.
    action = {
      ...action,
      'timestamp':
          (DateTime.tryParse(action['timestamp']?.toString() ?? '') ??
                  DateTime.now())
              .toUtc()
              .toIso8601String(),
    };
    final target = action['surfaceId'] as String;
    final models = <String, dynamic>{};
    for (final id in surfaceIds) {
      if (_sendData.contains(id) && channelOf(id) == channelOf(target)) {
        models[id] = contextForSurface(id).dataModel
            .getValue<Object?>(DataPath.root);
      }
    }
    return {
      'version': 'v0.9',
      'action': action,
      'metadata': {
        'a2uiClientDataModel': {'version': 'v0.9', 'surfaces': models},
      },
    };
  }

  void _submit(Json message) {
    if (message['error'] case final Map failure) {
      final renderId = failure['surfaceId'];
      final id = _renderScopes[renderId]?.surfaceId ?? renderId;
      if (id is String) {
        final detail = failure['message'].toString();
        if (surfaceErrors[id] == detail) return;
        surfaceErrors[id] = detail;
      } else {
        final fingerprint = jsonEncode(failure);
        if (!_unscopedErrors.add(fingerprint)) return;
        if (_unscopedErrors.length > 100) {
          _unscopedErrors.remove(_unscopedErrors.first);
        }
      }
      record('ERROR', message);
      // Surface may report during build; defer ChangeNotifier notifications.
      scheduleMicrotask(_emit);
      return;
    }
    if (!connected) throw StateError(messages.errorDisconnectedAction);
    final scope = _renderScopes[message['action']?['surfaceId']];
    final envelope = actionEnvelope(
      Map<String, dynamic>.from(message['action']),
    );
    record('OUT', envelope);
    {
      final native = _native;
      final epoch = _epoch;
      if (native == null) throw StateError(messages.errorStoppedHub);
      unawaited(
        (scope != null
                ? scope.send(envelope)
                : native is GatewayClient
                ? native.send(envelope)
                : Future<void>.error(StateError('Surface handle expired')))
            .catchError((Object e) {
              if (epoch == _epoch) {
                error = messages.errorActionFailed(describeFailure(e));
                _emit();
              }
            }),
      );
    }
    _emit();
  }

  Future<void> _stop() {
    _modals.clear();
    _epoch++;
    _closingNative ??= _native;
    _native = null;
    return _nativeStopped ??= _closeNative();
  }

  Future<void> _closeNative() async {
    try {
      // Keep ownership until close releases the native identity lease. A failed
      // attempt can be retried, but must never allow a competing client to start.
      await Future<void>.sync(() => _closingNative?.close());
      _closingNative = null;
    } finally {
      _nativeStopped = null;
    }
  }

  Future<void> startNative() async {
    if (_disposed) throw StateError('Hub session is disposed');
    final stopped = _stop();
    status = 'Starting';
    error = null;
    services = [];
    connections = [];
    log.clear();
    _clearRenderer();
    final epoch = _epoch;
    _emit();
    try {
      await stopped;
      if (epoch != _epoch) return;
      final native = gatewayClientFactory(
        store: nativeStore,
        onMessage: (message) {
          if (epoch == _epoch) receive(message);
        },
        onRetract: (route) {
          if (epoch != _epoch) return;
          for (final id
              in surfaceIds.where((id) => channelOf(id) == route).toList()) {
            receive({
              'version': 'v0.9',
              'deleteSurface': {'surfaceId': id},
            });
          }
        },
        onEvent: (event) {
          if (epoch == _epoch) {
            record('IROH', event);
            error = event;
            _emit();
          }
        },
      );
      _native = native;
      native.addListener(() {
        if (epoch != _epoch) return;
        services = native.services;
        connections = native.connections;
        status = native is RemoteClient
            ? switch (native.state) {
                RemoteConnectionState.connected => 'Connected',
                RemoteConnectionState.connecting => 'Starting',
                RemoteConnectionState.disconnected => 'Reconnecting',
                RemoteConnectionState.suspended => 'Suspended',
                RemoteConnectionState.failed => 'Unavailable',
                RemoteConnectionState.closed => 'Stopped',
              }
            : connections.any(
                (c) => c['status'].toString().toLowerCase() == 'connected',
              )
            ? 'Connected'
            : native.peers.isEmpty
            ? 'Ready'
            : 'Reconnecting';
        _emit();
      });
      await native.start();
      if (epoch == _epoch) _emit();
    } catch (e) {
      if (epoch == _epoch) {
        status = 'Unavailable';
        error = messages.errorHubFailed(describeFailure(e));
        _emit();
      }
      rethrow;
    }
  }

  Future<void> pairGateway(String invitation) async {
    if (_native == null || status == 'Unavailable') {
      await startNative();
    }
    try {
      await _gateway.pair(invitation);
      error = null;
    } catch (e) {
      error = messages.errorPairingFailed(describeFailure(e));
      _emit();
      rethrow;
    }
  }

  Future<void> forgetGateway(String id) async {
    for (final surface in surfaceIds.where((s) => s.startsWith('$id~'))) {
      _modals.remove(_surfaceRenderIds[surface] ?? surface);
    }
    await _gateway.forget(id);
  }

  void retryGateway(String id) => _native?.retry(id);
  Future<void> setNativeOnline(bool online) async {
    await _gateway.setOnline(online);
  }

  Future<void> suspendNative() async {
    _modals.clear();
    await _native?.suspend();
  }

  Future<void> resumeNative() async {
    await _native?.resume();
  }

  /// Await endpoint shutdown when replacing an app session or in integration tests.
  Future<void> stopNative() async {
    final stopped = _stop();
    final epoch = _epoch;
    await stopped;
    if (epoch != _epoch) return;
    services = [];
    connections = [];
    status = 'Stopped';
    if (!_disposed) _clearRenderer();
    _emit();
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    unawaited(
      _stop().catchError((Object failure) {
        // dispose cannot return a Future. Observe its shutdown failure without
        // changing the rejected Future returned to any awaiting start or stop.
        error = messages.errorHubFailed(describeFailure(failure));
        record('ERROR', error!);
      }),
    );
    _submits?.cancel();
    renderer.dispose();
    super.dispose();
  }
}
