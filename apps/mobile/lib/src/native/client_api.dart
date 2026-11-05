import 'package:flutter/foundation.dart';

import '../models.dart';
import 'protocol.dart';
import 'store.dart';

/// Rendering and connection lifecycle shared by local and remote Hubs.
abstract class HubClient extends ChangeNotifier {
  String? get identity;
  bool get online;
  List<GatewayPeer> get peers;
  List<HubService> get services;
  List<Json> get connections;
  Future<void> start();
  void retry(String id);

  /// Captured from the currently delivered render batch, never from action data.
  GatewaySurface? surfaceFor(String id) => null;
  bool get usesScopedSurfaces => false;
  Future<void> suspend() async {}
  Future<void> resume() async {}
  Future<void> close();
}

/// Administration is available only for the Hub on this device.
abstract class GatewayClient extends HubClient {
  Future<void> pair(String invitation);
  Future<void> forget(String id);
  Future<void> setOnline(bool value);
  Future<void> send(Json message);
  Future<Uint8List> resource(String route, String id);
}

/// This object belongs to one render subscription and one Surface generation.
abstract class GatewaySurface {
  String get renderId;
  String get surfaceId;
  Future<void> send(Json message);
  GatewayImageRequest image(String uri);
}

class GatewayImageRequest {
  GatewayImageRequest(this.bytes, this.cancel);
  final Future<Uint8List> bytes;
  final void Function() cancel;
}

typedef GatewayClientFactory = HubClient Function({
  required NativeHubStore store,
  required void Function(Json) onMessage,
  required void Function(String) onRetract,
  required void Function(String) onEvent,
});
