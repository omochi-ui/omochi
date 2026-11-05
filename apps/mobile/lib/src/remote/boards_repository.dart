import '../models.dart';
import '../native/client_remote.dart';
import 'board_models.dart';

/// Board operations shared by controllers and their in-memory test adapter.
abstract interface class BoardsRepository {
  bool get owner;
  bool get interact;
  bool get editLayout;
  Future<List<RemoteBoard>> list();
  Future<RemoteBoard> get(String id);
  Future<List<RemoteBoard>> presets();
  Future<void> save(RemoteBoard board, {required bool create});
  Future<void> delete(RemoteBoard board);
  Future<List<RemoteConnectionSummary>> connections();
  Future<SharingConfiguration> sharing();
  Future<List<RemoteShare>> shares();
  Future<String> createShare({
    required String boardId,
    required bool interact,
    required bool editLayout,
  });
  Future<void> revokeShare(String id);
}

/// JSON and operation names are confined to the native protocol adapter.
class RemoteBoardsRepository implements BoardsRepository {
  RemoteBoardsRepository(this.client);
  final RemoteClient client;
  @override
  bool get owner => client.owner;
  @override
  bool get interact => client.interact;
  @override
  bool get editLayout => client.editLayout;
  RemoteBoard get _shared =>
      client.welcome?.board ??
      (throw StateError('Shared board is unavailable'));
  List<RemoteBoard> _boards(Object? value) => (value as List)
      .map((value) => RemoteBoard.fromJson(value as Json))
      .toList();
  @override
  Future<List<RemoteBoard>> list() async => owner
      ? _boards(await client.control({'method': 'boardsList'}))
      : [_shared];
  @override
  Future<RemoteBoard> get(String id) async => owner
      ? RemoteBoard.fromJson(
          await client.control({'method': 'boardGet', 'id': id}) as Json,
        )
      : _shared;
  @override
  Future<List<RemoteBoard>> presets() async =>
      _boards(await client.control({'method': 'presets'}));
  @override
  Future<void> save(RemoteBoard board, {required bool create}) async {
    final value = board.toJson()..remove('id');
    if (create) value.remove('revision');
    if (owner) {
      await client.control({
        'method': create ? 'boardCreate' : 'boardUpdate',
        if (!create) 'id': board.id,
        'value': value,
      });
    } else {
      final response = await client.rpc({'op': 'layout', 'value': value});
      client.updateSharedBoard(RemoteBoard.fromJson(response['board'] as Json));
    }
  }

  @override
  Future<void> delete(RemoteBoard board) async {
    await client.control({
      'method': 'boardDelete',
      'id': board.id,
      'value': {'revision': board.revision},
    });
  }

  @override
  Future<List<RemoteConnectionSummary>> connections() async =>
      (await client.control({'method': 'connections'}) as List)
          .map((value) => RemoteConnectionSummary.fromJson(value as Json))
          .toList();
  @override
  Future<SharingConfiguration> sharing() async => SharingConfiguration.fromJson(
    await client.control({'method': 'sharing'}) as Json,
  );
  @override
  Future<List<RemoteShare>> shares() async =>
      (await client.control({'method': 'sharesList'}) as List)
          .map((value) => RemoteShare.fromJson(value as Json))
          .toList();
  @override
  Future<String> createShare({
    required String boardId,
    required bool interact,
    required bool editLayout,
  }) async {
    final result = await client.control({
      'method': 'shareCreate',
      'board_id': boardId,
      'permissions': {'interact': interact, 'editLayout': editLayout},
    }) as Json;
    return result['url'] as String;
  }

  @override
  Future<void> revokeShare(String id) async {
    await client.control({'method': 'shareRevoke', 'id': id});
  }
}
