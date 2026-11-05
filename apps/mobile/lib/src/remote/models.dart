import '../models.dart';
import 'board_models.dart';

enum RemoteAccessMode { share, owner }

class RemoteSource {
  const RemoteSource({
    required this.id,
    required this.hubId,
    required this.grantId,
    required this.mode,
    this.accepted = false,
  });
  factory RemoteSource.fromJson(Json value) => RemoteSource(
    id: value['id'] as String,
    hubId: value['hubId'] as String,
    grantId: value['grantId'] as String,
    mode: RemoteAccessMode.values.byName(value['mode'] as String),
    accepted: value['accepted'] == true,
  );
  final String id, hubId, grantId;
  final RemoteAccessMode mode;
  final bool accepted;
}

class RemotePermissions {
  const RemotePermissions({this.interact = false, this.editLayout = false});
  factory RemotePermissions.fromJson(Json value) => RemotePermissions(
    interact: value['interact'] == true,
    editLayout: value['editLayout'] == true,
  );
  final bool interact, editLayout;
}

class RemoteWelcome {
  const RemoteWelcome({
    this.services = const [],
    this.permissions = const RemotePermissions(),
    this.board,
  });
  factory RemoteWelcome.fromJson(Json value) => RemoteWelcome(
    services: (value['services'] as List? ?? const [])
        .map((v) => HubService.fromJson(Map<String, dynamic>.from(v as Map)))
        .toList(),
    permissions: RemotePermissions.fromJson(
      Map<String, dynamic>.from(value['permissions'] as Map? ?? const {}),
    ),
    board: value['board'] == null
        ? null
        : RemoteBoard.fromJson(
            Map<String, dynamic>.from(value['board'] as Map),
          ),
  );
  final List<HubService> services;
  final RemotePermissions permissions;
  final RemoteBoard? board;
}

enum RemoteConnectionState {
  disconnected,
  connecting,
  connected,
  suspended,
  failed,
  closed,
}

enum RemoteFrameKind { message, snapshotEnd }

class RemoteFrame {
  RemoteFrame.fromJson(Json value)
    : kind = RemoteFrameKind.values.byName(value['kind'] as String),
      sequence = value['sequence'] as String,
      lease = value['lease'] as String?,
      message = value['message'] == null
          ? null
          : Map<String, dynamic>.from(value['message'] as Map);
  final RemoteFrameKind kind;
  final String sequence;
  final String? lease;
  // A2UI renderer messages deliberately remain open JSON at the renderer seam.
  final Json? message;
}
