import 'package:flutter/foundation.dart';

import '../hub_session.dart';
import 'repository.dart';

/// Owns the handoff between a running Hub and connection selection.
/// Navigation supplies only the route presentation; endpoint ordering stays here.
class HubNavigationController extends ChangeNotifier {
  HubNavigationController(this.session, this.repository);

  final HubSession session;
  final RemoteRepository repository;
  Future<void> _ready = Future.value();
  Future<void> _transitions = Future.value();
  bool _choosing = false;
  bool _suspended = false;
  bool _foreground = true;
  bool _disposed = false;

  Future<void> get ready => _ready;
  bool get choosing => _choosing;
  Future<bool> get hasSelectedHub async => await repository.selected() != null;

  Future<void> _serialize(Future<void> Function() action) {
    final next = _transitions.then((_) => action());
    // One failed transition must not prevent shutdown or a later retry.
    _transitions = next.catchError((Object _) {});
    return next;
  }

  Future<void> start() => _ready = _serialize(() async {
    if (_disposed) return;
    await session.startNative();
    _suspended = false;
    if (!_foreground && !_choosing) await _setSuspended(true);
  });

  Future<void> _setSuspended(bool value) async {
    if (_suspended == value || _disposed) return;
    if (value) {
      // A failed suspension may already have stopped rendering/networking.
      _suspended = true;
      await session.suspendNative();
    } else {
      await session.resumeNative();
      _suspended = false;
    }
  }

  Future<void> setForeground(bool value) {
    _foreground = value;
    if (_choosing || _disposed) return Future.value();
    final started = _ready;
    return _serialize(() async {
      await started;
      await _setSuspended(!value);
    });
  }

  Future<void> _syncForeground() async {
    while (!_disposed && _suspended != !_foreground) {
      await _setSuspended(!_foreground);
    }
  }

  Future<void> choose({
    required void Function() prepareRoute,
    required Future<void> Function() present,
  }) async {
    if (_choosing || _disposed) return;
    _choosing = true;
    notifyListeners();
    try {
      await _ready;
      if (_disposed) return;
      if (!(await (session.remoteLeaveGuard?.call() ?? Future.value(true)))) {
        return;
      }
      if (_disposed) return;
      prepareRoute();
      final remote = session.remoteClient != null;
      await _serialize(() async {
        if (remote) {
          await session.stopNative();
        } else {
          await _setSuspended(true);
        }
      });
      if (_disposed) return;
      try {
        await present();
      } finally {
        if (!_disposed) {
          await _serialize(() async {
            if (remote) {
              await session.startNative();
              _suspended = false;
            }
            await _setSuspended(!_foreground);
          });
        }
      }
    } finally {
      try {
        if (!_disposed) {
          // OS events also arrive while an unsaved-draft dialog is open. Even a
          // declined handoff must apply the latest foreground state.
          await _serialize(_syncForeground);
        }
      } finally {
        _choosing = false;
        if (!_disposed) notifyListeners();
      }
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}
