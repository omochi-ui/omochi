import 'package:flutter/foundation.dart';

import '../hub_session.dart';
import '../native/client_remote.dart';
import 'models.dart';
import 'repository.dart';

class RemoteConnectionsController extends ChangeNotifier {
  RemoteConnectionsController(this.repository);
  factory RemoteConnectionsController.native() =>
      RemoteConnectionsController(NativeRemoteRepository.mobile());
  final RemoteRepository repository;
  List<RemoteSource> records = const [];
  Object? error;
  bool busy = false;
  bool _disposed = false;
  Future<void> _operations = Future.value();
  void _emit() {
    if (!_disposed) notifyListeners();
  }

  Future<T> _run<T>(Future<T> Function() work) {
    final result = _operations.then((_) async {
      if (_disposed) throw StateError('Connections controller is disposed');
      busy = true;
      error = null;
      _emit();
      try {
        return await work();
      } catch (e) {
        error = e;
        rethrow;
      } finally {
        busy = false;
        _emit();
      }
    });
    _operations = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return result;
  }

  Future<void> load() => _run(() async {
    records = await repository.list();
  });
  Future<RemoteSource> parse(String invitation) =>
      _run(() => repository.parse(invitation));
  Future<void> register(String invitation, {bool save = true}) =>
      _run(() async {
        await repository.register(invitation, save: save);
        records = await repository.list();
      });
  Future<void> forget(String id) => _run(() async {
    await repository.forget(id);
    records = await repository.list();
  });
  Future<RemoteSource?> selectedSource() => _run(() async {
    final id = await repository.selected();
    return records.where((source) => source.id == id).firstOrNull;
  });
  Future<void> select(RemoteSource? source) =>
      _run(() => repository.select(source?.id));
  Future<void> open(
    RemoteSource source, {
    String? invitation,
    required Future<void> Function(HubSession session) present,
  }) => _run(() async {
    if (invitation == null) await repository.select(source.id);
    final session = HubSession(
      gatewayClientFactory:
          ({
            required store,
            required onMessage,
            required onRetract,
            required onEvent,
          }) => RemoteClient(
            store,
            source,
            onMessage,
            onRetract,
            onEvent,
            invitation: invitation,
            repository: repository,
          ),
    );
    try {
      await present(session);
    } finally {
      try {
        await session.stopNative();
      } finally {
        session.dispose();
      }
    }
    records = await repository.list();
  });
  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
