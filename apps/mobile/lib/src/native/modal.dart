import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:genui/genui.dart';

/// One registry per renderer; each token belongs to one Surface generation.
class NativeModalRegistry {
  final _surfaces = <String, _SurfaceModals>{};

  void create(String id) {
    remove(id);
    _surfaces[id] = _SurfaceModals();
  }

  void remove(String id) => _surfaces.remove(id)?.invalidate();

  void clear() {
    final owners = _surfaces.values.toList();
    _surfaces.clear();
    for (final owner in owners) {
      owner.invalidate();
    }
  }

  CatalogItem get catalogItem => CatalogItem(
    name: BasicCatalogItems.modal.name,
    dataSchema: BasicCatalogItems.modal.dataSchema,
    widgetBuilder: (item) => _NativeModal(
      key: ObjectKey(_surfaces[item.surfaceId]),
      owner: _surfaces[item.surfaceId],
      item: item,
    ),
  );
}

class _SurfaceModals {
  bool valid = true;
  final routes = <ModalBottomSheetRoute<void>>{};

  void invalidate() {
    valid = false; // Expire callbacks before scheduling Navigator mutations.
    final closing = routes.toList();
    routes.clear();
    for (final route in closing) {
      _removeRoute(route);
    }
  }
}

void _removeRoute(ModalBottomSheetRoute<void> route) {
  // dispose can run while Navigator is locked or the widget tree is building.
  // Always defer: even SchedulerPhase.idle can occur in Navigator callbacks.
  WidgetsBinding.instance.addPostFrameCallback((_) {
    final navigator = route.navigator;
    if (navigator != null && navigator.mounted && route.isActive) {
      navigator.removeRoute(route);
    }
  });
  SchedulerBinding.instance.ensureVisualUpdate();
}

class _NativeModal extends StatefulWidget {
  const _NativeModal({super.key, required this.owner, required this.item});
  final _SurfaceModals? owner;
  final CatalogItemContext item;

  @override
  State<_NativeModal> createState() => _NativeModalState();
}

class _NativeModalState extends State<_NativeModal> {
  ModalBottomSheetRoute<void>? _route;

  @override
  void dispose() {
    final route = _route;
    if (route != null) {
      widget.owner?.routes.remove(route);
      _removeRoute(route);
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final item = widget.item;
    final data = item.data as Map;
    final owner = widget.owner;
    return ElevatedButton(
      onPressed: () {
        if (!mounted ||
            owner == null ||
            !owner.valid ||
            !identical(owner, widget.owner) ||
            _route != null) {
          return;
        }
        final navigator = Navigator.of(context);
        final localizations = MaterialLocalizations.of(context);
        final route = ModalBottomSheetRoute<void>(
          isScrollControlled: true,
          useSafeArea: true,
          capturedThemes: InheritedTheme.capture(
            from: context,
            to: navigator.context,
          ),
          barrierLabel: localizations.scrimLabel,
          modalBarrierColor: Theme.of(context)
              .bottomSheetTheme
              .modalBarrierColor,
          builder: (context) {
            // A Surface can expire after push but before the route's first build.
            if (!mounted || !owner.valid) return const SizedBox.shrink();
            return Padding(
              padding: EdgeInsets.only(
                bottom: MediaQuery.viewInsetsOf(context).bottom,
              ),
              child: SafeArea(
                child: item.buildChild(data['content'] as String),
              ),
            );
          },
        );
        _route = route;
        owner.routes.add(route);
        navigator.push(route).whenComplete(() {
          owner.routes.remove(route);
          if (identical(_route, route)) _route = null;
        });
      },
      child: item.buildChild(data['trigger'] as String),
    );
  }
}
