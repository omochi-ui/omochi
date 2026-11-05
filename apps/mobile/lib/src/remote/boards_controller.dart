import 'dart:async';

import 'package:flutter/foundation.dart';

import '../native/bridge.dart';
import 'board_models.dart';
import 'boards_repository.dart';

/// Polling belongs to the connection-facing controller, never the Widget tree.
class BoardsController extends ChangeNotifier {
  BoardsController(this.repository);
  final BoardsRepository repository;
  List<RemoteBoard> boards = const [];
  Object? error;
  bool _disposed = false, _fetching = false;
  Timer? _timer;
  void start() {
    unawaited(refresh());
    _timer ??= Timer.periodic(const Duration(seconds: 1), (_) => refresh());
  }

  Future<void> refresh() async {
    if (_disposed || _fetching) return;
    _fetching = true;
    try {
      final latest = await repository.list();
      if (!_disposed) {
        boards = latest;
        error = null;
      }
    } catch (failure) {
      if (!_disposed) error = failure;
    } finally {
      _fetching = false;
      if (!_disposed) notifyListeners();
    }
  }

  Future<T?> _inspect<T>(Future<T> Function() operation) async {
    if (_disposed) return null;
    try {
      return await operation();
    } catch (failure) {
      if (!_disposed) {
        error = failure;
        notifyListeners();
      }
      return null;
    }
  }

  Future<List<RemoteBoard>?> presets() => _inspect(repository.presets);
  Future<List<RemoteConnectionSummary>?> connections() =>
      _inspect(repository.connections);
  Future<SharingConfiguration?> sharing() => _inspect(repository.sharing);

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    super.dispose();
  }
}

class BoardViewController extends ChangeNotifier {
  BoardViewController(this.repository, this.board);
  final BoardsRepository repository;
  RemoteBoard board;
  Object? error;
  bool _disposed = false, _fetching = false;
  Timer? _timer;
  void start() {
    _timer ??= Timer.periodic(const Duration(seconds: 1), (_) => refresh());
  }

  Future<void> refresh() async {
    if (_disposed || _fetching || board.id == null) return;
    _fetching = true;
    try {
      final latest = await repository.get(board.id!);
      if (!_disposed) {
        board = latest;
        error = null;
      }
    } catch (failure) {
      if (!_disposed) error = failure;
    } finally {
      _fetching = false;
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    _timer?.cancel();
    super.dispose();
  }
}

enum PanelDimension { column, row, width, height }

class PanelDraft {
  PanelDraft(BoardPanel panel)
    : id = panel.id,
      title = panel.title,
      reference = panel.reference,
      x = panel.x,
      y = panel.y,
      w = panel.w,
      h = panel.h;
  final String id;
  String title;
  final SurfaceReference reference;
  int? x, y, w, h;
  int? dimension(PanelDimension dimension) => switch (dimension) {
    PanelDimension.column => x,
    PanelDimension.row => y,
    PanelDimension.width => w,
    PanelDimension.height => h,
  };
  void setDimension(PanelDimension dimension, int? value) {
    switch (dimension) {
      case PanelDimension.column:
        x = value;
      case PanelDimension.row:
        y = value;
      case PanelDimension.width:
        w = value;
      case PanelDimension.height:
        h = value;
    }
  }

  BoardPanel snapshot() => BoardPanel(
    id: id,
    title: title,
    reference: reference,
    x: x!,
    y: y!,
    w: w!,
    h: h!,
  );
}

/// Flutter owns the editable draft; remote operations lock it until completion.
/// Failed writes never replace the draft.
class BoardEditorController extends ChangeNotifier {
  BoardEditorController(
    this.repository,
    RemoteBoard board, {
    this.create = false,
  }) {
    _replace(board);
  }
  final BoardsRepository repository;
  final bool create;
  late RemoteBoard _base;
  late String name;
  late List<PanelDraft> _panels;
  List<PanelDraft> get panels => List.unmodifiable(_panels);
  int? get revision => _base.revision;
  bool dirty = false, busy = false, _disposed = false;
  Object? error;
  bool get conflict =>
      error is BridgeException && (error as BridgeException).code == 'Conflict';
  void _replace(RemoteBoard board) {
    _base = board;
    name = board.name;
    _panels = board.panels.map(PanelDraft.new).toList();
    dirty = false;
    error = null;
  }

  void _changed() {
    dirty = true;
    notifyListeners();
  }

  void rename(String value) {
    if (busy || _disposed) return;
    name = value;
    _changed();
  }

  void renamePanel(PanelDraft panel, String value) {
    if (busy || _disposed) return;
    panel.title = value;
    _changed();
  }

  void resizePanel(PanelDraft panel, PanelDimension dimension, String value) {
    if (busy || _disposed) return;
    panel.setDimension(dimension, int.tryParse(value));
    _changed();
  }

  void addPanel(String surfaceId) {
    if (busy || _disposed || !repository.owner) return;
    final reference = SurfaceReference.fromSurfaceId(surfaceId);
    final rows = _panels.fold(
      0,
      (rows, panel) => rows > (panel.y ?? 0) + (panel.h ?? 0)
          ? rows
          : (panel.y ?? 0) + (panel.h ?? 0),
    );
    _panels.add(
      PanelDraft(
        BoardPanel(
          id: 'panel-${DateTime.now().microsecondsSinceEpoch}',
          title: reference.surfaceId,
          reference: reference,
          x: 0,
          y: rows,
          w: 12,
          h: 4,
        ),
      ),
    );
    _changed();
  }

  void removePanel(PanelDraft panel) {
    if (busy || _disposed || !repository.owner) return;
    _panels.remove(panel);
    _changed();
  }

  Future<bool> save({bool delete = false}) async {
    if (busy || _disposed) return false;
    busy = true;
    error = null;
    notifyListeners();
    try {
      if (delete) {
        await repository.delete(_base);
      } else {
        await repository.save(
          RemoteBoard(
            id: _base.id,
            revision: _base.revision,
            name: name,
            panels: _panels.map((panel) => panel.snapshot()).toList(),
          ),
          create: create,
        );
      }
      if (!_disposed) dirty = false;
      return true;
    } catch (failure) {
      if (!_disposed) error = failure;
      return false;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  Future<void> reload() async {
    if (busy || _disposed || _base.id == null) return;
    busy = true;
    notifyListeners();
    try {
      final latest = await repository.get(_base.id!);
      if (!_disposed) _replace(latest);
    } catch (failure) {
      if (!_disposed) error = failure;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  void allowLeave() {
    dirty = false;
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

class SharesController extends ChangeNotifier {
  SharesController(this.repository);
  final BoardsRepository repository;
  List<RemoteShare> shares = const [];
  Object? error;
  bool busy = false, _disposed = false;
  Future<void> refresh() async {
    if (_disposed) return;
    try {
      final latest = await repository.shares();
      if (!_disposed) {
        shares = latest;
        error = null;
      }
    } catch (failure) {
      if (!_disposed) error = failure;
    }
    if (!_disposed) notifyListeners();
  }

  Future<String?> create({
    required String boardId,
    required bool public,
    required bool interact,
    required bool editLayout,
  }) async {
    if (busy || _disposed) return null;
    busy = true;
    error = null;
    notifyListeners();
    try {
      final link = await repository.createShare(
        boardId: boardId,
        public: public,
        interact: interact,
        editLayout: editLayout,
      );
      await refresh();
      return link;
    } catch (failure) {
      if (!_disposed) error = failure;
      return null;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  Future<void> revoke(String id) async {
    if (busy || _disposed) return;
    busy = true;
    notifyListeners();
    try {
      await repository.revokeShare(id);
      await refresh();
    } catch (failure) {
      if (!_disposed) error = failure;
    } finally {
      busy = false;
      if (!_disposed) notifyListeners();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
