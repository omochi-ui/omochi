/// A single link subscription dispatches to the visible Hub/chooser. Gateway
/// enrollment always belongs to the local Hub, even while a remote Hub is open.
class InvitationRouting {
  InvitationRouting(this.onGateway);

  final Future<void> Function(String) onGateway;
  final _routes = <InvitationRoute>[];

  InvitationRoute enter(
    Future<void> Function(String) onHub, {
    Future<bool> Function()? mayLeave,
  }) {
    final route = InvitationRoute(onHub, mayLeave);
    _routes.add(route);
    return route;
  }

  void leave(InvitationRoute route) => _routes.remove(route);

  Future<void> dispatch(String input) async {
    if (isHubInvitation(input)) {
      if (_routes.isNotEmpty) await _routes.last.onHub(input);
    } else {
      await onGateway(input);
    }
  }

  Future<bool> mayReturnToLocal() async {
    for (final route in _routes.reversed.toList()) {
      if (!(await (route.mayLeave?.call() ?? Future.value(true)))) return false;
    }
    return true;
  }
}

class InvitationRoute {
  InvitationRoute(this.onHub, this.mayLeave);
  final Future<void> Function(String) onHub;
  final Future<bool> Function()? mayLeave;
}

bool isHubInvitation(String input) {
  final uri = Uri.tryParse(input);
  return uri != null &&
      ((uri.scheme == 'omochi' && ['share', 'owner'].contains(uri.host)) ||
          (uri.scheme == 'https' &&
              (uri.fragment.startsWith('share:') ||
                  uri.fragment.startsWith('owner:'))));
}
