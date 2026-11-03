import 'package:flutter/foundation.dart';

import '../models.dart';
import 'protocol.dart';
import 'store.dart';

abstract class GatewayClient extends ChangeNotifier {
  String? get identity;
  bool get online;
  List<GatewayPeer> get peers;
  List<HubService> get services;
  List<Json> get connections;
  Future<void> start();
  Future<void> pair(String invitation);
  Future<void> forget(String id);
  Future<void> setOnline(bool value);
  void retry(String id);
  Future<void> send(Json message);
  Future<Uint8List> resource(String route, String id);

  /// Captured from the currently delivered render batch, never from action data.
  GatewaySurface? surfaceFor(String id) => null;
  bool get usesScopedSurfaces => false;
  Future<void> suspend() async {}
  Future<void> resume() async {}
  Future<void> close();
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

typedef GatewayClientFactory = GatewayClient Function({
  required NativeHubStore store,
  required void Function(Json) onMessage,
  required void Function(String) onRetract,
  required void Function(String) onEvent,
});
