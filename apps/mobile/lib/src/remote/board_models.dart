import '../models.dart';

class SurfaceReference {
  const SurfaceReference(this.nodeId, this.serviceId, this.surfaceId);
  final String nodeId, serviceId, surfaceId;
  factory SurfaceReference.fromJson(Json value) => SurfaceReference(
    value['nodeId'] as String,
    value['serviceId'] as String,
    value['surfaceId'] as String,
  );
  factory SurfaceReference.fromSurfaceId(String id) {
    final node = id.indexOf('~'), surface = id.indexOf('/');
    if (node <= 0 || surface <= node + 1 || surface == id.length - 1) {
      throw const FormatException('Invalid surface reference');
    }
    return SurfaceReference(
      id.substring(0, node),
      id.substring(node + 1, surface),
      id.substring(surface + 1),
    );
  }
  String get id => '$nodeId~$serviceId/$surfaceId';
  Json toJson() => {
    'nodeId': nodeId,
    'serviceId': serviceId,
    'surfaceId': surfaceId,
  };
}

class BoardPanel {
  const BoardPanel({
    required this.id,
    required this.title,
    required this.reference,
    required this.x,
    required this.y,
    required this.w,
    required this.h,
  });
  final String id, title;
  final SurfaceReference reference;
  final int x, y, w, h;
  factory BoardPanel.fromJson(Json value) => BoardPanel(
    id: value['id'] as String,
    title: value['title'] as String,
    reference: SurfaceReference.fromJson(value['reference'] as Json),
    x: value['x'] as int,
    y: value['y'] as int,
    w: value['w'] as int,
    h: value['h'] as int,
  );
  Json toJson() => {
    'id': id,
    'title': title,
    'reference': reference.toJson(),
    'x': x,
    'y': y,
    'w': w,
    'h': h,
  };
}

class RemoteBoard {
  RemoteBoard({
    this.id,
    this.revision,
    required this.name,
    required List<BoardPanel> panels,
  }) : panels = List.unmodifiable(panels);
  final String? id;
  final int? revision;
  final String name;
  final List<BoardPanel> panels;
  factory RemoteBoard.empty() => RemoteBoard(name: '', panels: const []);
  factory RemoteBoard.fromJson(Json value) => RemoteBoard(
    id: value['id'] as String?,
    revision: value['revision'] as int?,
    name: value['name'] as String,
    panels: (value['panels'] as List)
        .map((panel) => BoardPanel.fromJson(panel as Json))
        .toList(),
  );
  int get rows => panels.fold(
    1,
    (rows, panel) => rows > panel.y + panel.h ? rows : panel.y + panel.h,
  );
  Json toJson() => {
    if (id != null) 'id': id,
    if (revision != null) 'revision': revision,
    'name': name,
    'panels': panels.map((panel) => panel.toJson()).toList(),
  };
}

class RemoteShare {
  const RemoteShare({
    required this.id,
    required this.boardId,
    required this.interact,
    required this.editLayout,
  });
  final String id, boardId;
  final bool interact, editLayout;
  factory RemoteShare.fromJson(Json value) {
    final permissions = value['permissions'] as Json;
    return RemoteShare(
      id: value['id'] as String,
      boardId: value['boardId'] as String,
      interact: permissions['interact'] as bool,
      editLayout: permissions['editLayout'] as bool,
    );
  }
}

class RemoteConnectionSummary {
  const RemoteConnectionSummary(this.name, this.status);
  final String name, status;
  factory RemoteConnectionSummary.fromJson(Json value) =>
      RemoteConnectionSummary(
        (value['name'] ?? value['nodeId']) as String,
        value['status'] as String,
      );
}

class SharingConfiguration {
  SharingConfiguration(this.enabled, this.hubId, List<String> relayUrls)
    : relayUrls = List.unmodifiable(relayUrls);
  final bool enabled;
  final String? hubId;
  final List<String> relayUrls;
  factory SharingConfiguration.fromJson(Json value) => SharingConfiguration(
    value['enabled'] != false,
    value['hubId'] as String?,
    (value['relayUrls'] as List? ?? const []).cast<String>(),
  );
}
