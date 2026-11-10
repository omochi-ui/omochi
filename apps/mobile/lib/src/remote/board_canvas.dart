import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:genui/genui.dart';
import 'package:sliver_dashboard/sliver_dashboard.dart' as grid;

import '../hub_session.dart';
import '../messages.dart';
import 'board_models.dart';
import 'board_layout.dart';
import 'board_surface.dart';

class BoardCanvas extends StatefulWidget {
  const BoardCanvas({
    super.key,
    required this.panels,
    required this.session,
    required this.interact,
    this.onChange,
  });
  final List<BoardPanel> panels;
  final HubSession session;
  final bool interact;
  final ValueChanged<List<BoardPanel>>? onChange;
  @override
  State<BoardCanvas> createState() => _BoardCanvasState();
}

class _BoardCanvasState extends State<BoardCanvas> with WidgetsBindingObserver {
  late final grid.DashboardController controller;
  final surfaces = <String, BoardSurfaceAnchor>{};
  String? dragItemId;
  final scrollController = ScrollController();
  final sliverKey = GlobalKey();
  final reorderKey = GlobalKey<SliverReorderableListState>();
  final gridPointers = <int>{};
  bool gridInteraction = false;
  bool reordering = false;
  final redraw = ValueNotifier(0);
  bool importing = false;
  double? previousWidth;
  bool get editing => widget.onChange != null;
  List<grid.LayoutItem> get layout => [
    for (final p in widget.panels)
      grid.LayoutItem(
        id: p.id,
        x: p.x,
        y: p.y,
        w: p.w,
        h: p.h,
        minW: 1,
        maxW: 12,
        minH: 1,
        maxH: 1000,
      ),
  ];
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    HardwareKeyboard.instance.addHandler(handleKey);
    scrollController.addListener(refreshSurfaces);
    controller = grid.DashboardController(
      initialSlotCount: 12,
      initialLayout: layout,
      maxHistoryLength: 0,
      onLayoutChanged: (items, slots) {
        if (importing || !editing || slots != 12) return;
        final byId = {for (final i in items) i.id: i};
        if (byId.length != widget.panels.length ||
            !widget.panels.every((p) => byId.containsKey(p.id))) {
          return;
        }
        final next = [
          for (final p in widget.panels)
            geometry(
              p,
              byId[p.id]!.x,
              byId[p.id]!.y,
              byId[p.id]!.w,
              byId[p.id]!.h,
            ),
        ];
        widget.onChange?.call(next);
        if (!validLayout(next)) sync();
      },
    );
    controller.policy = _HandleDragPolicy((id) => editing && id == dragItemId);
    sync();
  }

  void refreshSurfaces() {
    for (final anchor in surfaces.values) {
      anchor.refresh();
    }
  }

  void sync() {
    importing = true;
    cancelInteraction();
    dragItemId = null;
    controller.setCompactionType(grid.CompactType.none);
    controller.importLayout([for (final i in layout) i.toMap()]);
    controller.setCompactionType(
      editing ? grid.CompactType.vertical : grid.CompactType.none,
    );
    // Strategy changes compact immediately; imports must preserve saved geometry.
    controller.layout.value = layout;
    controller.setEditMode(
      editing && (previousWidth == null || previousWidth! > 760),
    );
    importing = false;
  }

  @override
  void didUpdateWidget(BoardCanvas oldWidget) {
    super.didUpdateWidget(oldWidget);
    sync();
    redraw.value++;
    surfaces.removeWhere((id, anchor) {
      if (widget.panels.any((p) => p.id == id)) return false;
      anchor.dispose();
      return true;
    });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    HardwareKeyboard.instance.removeHandler(handleKey);
    redraw.dispose();
    scrollController.dispose();
    for (final anchor in surfaces.values) {
      anchor.dispose();
    }
    controller.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) cancelInteraction();
  }

  void cancelInteraction() {
    // Restore the draft before the overlay receives pointer-cancel: its up
    // handler can then clear its session without committing a partial resize.
    controller.cancelInteraction();
    dragItemId = null;
    for (final pointer in gridPointers) {
      // GestureBinding guarantees cancellation precedes the next pointer event.
      GestureBinding.instance.cancelPointer(pointer);
    }
    gridPointers.clear();
    gridInteraction = false;
    if (reordering) {
      reordering = false;
      reorderKey.currentState?.cancelReorder();
    }
  }

  bool handleKey(KeyEvent event) {
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.escape &&
        (gridPointers.isNotEmpty ||
            controller.isDragging.value ||
            reordering)) {
      // Package item shortcuts consume Escape before ancestor shortcuts and
      // only recognize dragging. Also tear down native resize pointer sessions.
      cancelInteraction();
      return true;
    }
    return false;
  }

  Widget narrowHandle({required int index, required Widget child}) =>
      switch (Theme.of(context).platform) {
        TargetPlatform.android || TargetPlatform.iOS =>
          ReorderableDelayedDragStartListener(index: index, child: child),
        _ => ReorderableDragStartListener(index: index, child: child),
      };

  void reorder(int oldIndex, int newIndex) {
    final ordered = readingOrder(widget.panels);
    if (newIndex == oldIndex || newIndex < 0 || newIndex >= ordered.length) {
      return;
    }
    final item = ordered.removeAt(oldIndex);
    ordered.insert(newIndex, item);
    final next = packPanels(ordered);
    widget.onChange?.call(next);
  }

  Future<void> size(BoardPanel panel) async {
    final m = messagesFor(context);
    var w = panel.w, h = panel.h;
    final result = await showModalBottomSheet<List<BoardPanel>>(
      context: context,
      isScrollControlled: true,
      builder: (context) => StatefulBuilder(
        builder: (context, update) {
          final preview = packPanels(
            readingOrder(widget.panels)
                .map((p) => p.id == panel.id ? geometry(p, p.x, p.y, w, h) : p)
                .toList(),
          );
          final rows = preview.fold(
            1,
            (int v, p) => v > p.y + p.h ? v : p.y + p.h,
          );
          return SafeArea(
            child: SingleChildScrollView(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      m.boardSize,
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                    Wrap(
                      spacing: 8,
                      children: [
                        for (final option in [
                          (4, m.boardCompact),
                          (6, m.boardStandard),
                          (8, m.boardWide),
                          (12, m.boardFull),
                        ])
                          ChoiceChip(
                            label: Text(option.$2),
                            selected: w == option.$1,
                            onSelected: (_) => update(() => w = option.$1),
                          ),
                      ],
                    ),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        TextButton(
                          onPressed: h > 1 ? () => update(() => h--) : null,
                          child: Text(m.boardShorter),
                        ),
                        TextButton(
                          onPressed: h < 1000 ? () => update(() => h++) : null,
                          child: Text(m.boardTaller),
                        ),
                      ],
                    ),
                    Text(m.boardPreview),
                    SizedBox(
                      height: 160,
                      child: LayoutBuilder(
                        builder: (context, c) => Stack(
                          children: [
                            for (final p in preview)
                              Positioned(
                                left: p.x * c.maxWidth / 12,
                                top: p.y * 160 / rows,
                                width: p.w * c.maxWidth / 12,
                                height: p.h * 160 / rows,
                                child: Container(
                                  margin: const EdgeInsets.all(1),
                                  decoration: BoxDecoration(
                                    color: p.id == panel.id
                                        ? Theme.of(context).colorScheme.primary
                                        : Theme.of(context)
                                              .colorScheme
                                              .surfaceContainerHighest,
                                    borderRadius: BorderRadius.circular(3),
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.end,
                      children: [
                        TextButton(
                          onPressed: () => Navigator.pop(context),
                          child: Text(m.cancel),
                        ),
                        FilledButton(
                          onPressed: validLayout(preview)
                              ? () => Navigator.pop(context, preview)
                              : null,
                          child: Text(m.boardApply),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ),
          );
        },
      ),
    );
    if (result != null && mounted && editing) widget.onChange?.call(result);
  }

  BoardSurfaceAnchor anchor(String id) =>
      surfaces.putIfAbsent(id, BoardSurfaceAnchor.new);

  double headerHeight(double height) => height < 96 ? height / 2 : 48;

  double narrowHeight(BoardPanel panel) =>
      (panel.h * 56 - 16).clamp(320, 600).toDouble();

  Widget handle(BoardPanel panel, int index, {required bool wide}) {
    final grip = Semantics(
      label: messagesFor(context).boardMove,
      child: const Padding(
        padding: EdgeInsets.all(12),
        child: Icon(Icons.drag_handle),
      ),
    );
    if (!wide) return narrowHandle(index: index, child: grip);
    return Builder(
      builder: (context) => Listener(
        onPointerDown: (event) {
          if (!editing) return;
          dragItemId = panel.id;
          grid.DashboardOverlayProvider.maybeOf(context)
              ?.startDragging(panel.id, event.position);
        },
        onPointerUp: (_) => dragItemId = null,
        onPointerCancel: (_) => dragItemId = null,
        child: Tooltip(message: messagesFor(context).boardMove, child: grip),
      ),
    );
  }

  Widget card(BoardPanel panel, {required bool wide}) => ValueListenableBuilder(
    valueListenable: redraw,
    builder: (context, _, _) {
      final p = widget.panels.firstWhere(
        (p) => p.id == panel.id,
        orElse: () => panel,
      );
      final order = readingOrder(widget.panels);
      final index = order.indexWhere((item) => item.id == p.id);
      return Card(
        margin: EdgeInsets.zero,
        clipBehavior: Clip.antiAlias,
        child: LayoutBuilder(
          builder: (context, bounds) => Column(
            children: [
              SizedBox(
                height: headerHeight(bounds.maxHeight),
                child: ClipRect(
                  child: SingleChildScrollView(
                    scrollDirection: Axis.horizontal,
                    child: SizedBox(
                      width: bounds.maxWidth < (editing ? 280 : 96)
                          ? (editing ? 280.0 : 96.0)
                          : bounds.maxWidth,
                      child: Row(
                        children: [
                          if (editing) ...[
                            handle(p, index, wide: wide),
                            IconButton(
                              tooltip: messagesFor(context).boardUp,
                              onPressed: index > 0
                                  ? () => reorder(index, index - 1)
                                  : null,
                              icon: const Icon(Icons.arrow_upward),
                            ),
                            IconButton(
                              tooltip: messagesFor(context).boardDown,
                              onPressed: index >= 0 && index < order.length - 1
                                  ? () => reorder(index, index + 1)
                                  : null,
                              icon: const Icon(Icons.arrow_downward),
                            ),
                          ],
                          Expanded(
                            child: Padding(
                              padding: const EdgeInsets.all(8),
                              child: Text(
                                p.title,
                                maxLines: 2,
                                overflow: TextOverflow.ellipsis,
                              ),
                            ),
                          ),
                          if (editing)
                            IconButton(
                              tooltip: messagesFor(context).boardSize,
                              onPressed: () => size(p),
                              icon: const Icon(Icons.aspect_ratio),
                            ),
                        ],
                      ),
                    ),
                  ),
                ),
              ),
              Expanded(child: BoardSurfaceSlot(anchor: anchor(p.id))),
            ],
          ),
        ),
      );
    },
  );

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, c) {
      if (previousWidth != c.maxWidth) {
        cancelInteraction();
        previousWidth = c.maxWidth;
      }
      controller.isDragging.watch(context);
      refreshSurfaces();
      final ordered = readingOrder(widget.panels);
      final wide = c.maxWidth > 760;
      var narrowTop = 16.0;
      for (final p in ordered) {
        anchor(p.id).contentOrigin = wide
            ? Offset(
                16 + p.x * ((c.maxWidth - 32 - 11 * 16) / 12 + 16),
                16 + p.y * 56 + headerHeight(p.h * 56 - 16),
              )
            : Offset(16, narrowTop + headerHeight(narrowHeight(p)));
        narrowTop += narrowHeight(p) + 16;
      }
      // Match DashboardOverlay: mobile has no native edge resizing with
      // DragStartGesture.none, so its app content keeps the whole body.
      final resizeEdges =
          editing &&
          wide &&
          defaultTargetPlatform != TargetPlatform.android &&
          defaultTargetPlatform != TargetPlatform.iOS;
      controller.setEditMode(editing && wide);
      final aspectRatio = wide ? ((c.maxWidth - 32 - 11 * 16) / 12) / 40 : 1.0;
      Widget gridCard(BuildContext context, grid.LayoutItem item) =>
          card(widget.panels.firstWhere((p) => p.id == item.id), wide: true);
      return Listener(
        onPointerDown: (event) {
          // The overlay starts edge resizes after descendants see pointer-down.
          if (gridInteraction) gridPointers.add(event.pointer);
        },
        onPointerUp: (event) => gridPointers.remove(event.pointer),
        onPointerCancel: (event) => gridPointers.remove(event.pointer),
        child: CallbackShortcuts(
          bindings: {
            const SingleActivator(LogicalKeyboardKey.escape): cancelInteraction,
          },
          child: grid.DashboardOverlay(
            controller: controller,
            scrollController: scrollController,
            sliverKey: sliverKey,
            gridStyle: wide ? const grid.GridStyle() : null,
            fillViewport: wide,
            padding: const EdgeInsets.all(16),
            slotAspectRatio: aspectRatio,
            mainAxisSpacing: 16,
            crossAxisSpacing: 16,
            dragStartGesture: grid.DragStartGesture.none,
            onWillDelete: (_) async => false,
            onItemDragStart: (_) => gridInteraction = true,
            onItemResizeStart: (_) => gridInteraction = true,
            onItemDragEnd: (_) => gridInteraction = false,
            onItemResizeEnd: (_) => gridInteraction = false,
            itemBuilder: gridCard,
            itemFeedbackBuilder: (context, item, child) => Material(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  widget.panels.firstWhere((p) => p.id == item.id).title,
                ),
              ),
            ),
            child: Listener(
              onPointerCancel: (event) {
                // Descendants receive cancel before DashboardOverlay, which
                // treats it as release. Roll back before its commit handler.
                if (gridPointers.remove(event.pointer)) cancelInteraction();
              },
              child: BoardScrollView(
                controller: scrollController,
                slivers: [
                  SliverPadding(
                    padding: const EdgeInsets.all(16),
                    sliver: wide
                        ? grid.SliverDashboard(
                            key: sliverKey,
                            controller: controller,
                            slotAspectRatio: aspectRatio,
                            mainAxisSpacing: 16,
                            crossAxisSpacing: 16,
                            itemBuilder: gridCard,
                          )
                        : SliverReorderableList(
                            key: reorderKey,
                            itemCount: ordered.length,
                            onReorderItem: reorder,
                            onReorderStart: (_) {
                              reordering = true;
                              refreshSurfaces();
                            },
                            onReorderEnd: (_) {
                              reordering = false;
                              refreshSurfaces();
                            },
                            // The app stays in its stable host during the drag.
                            proxyDecorator: (child, index, animation) =>
                                Material(
                                  child: Center(
                                    child: Text(ordered[index].title),
                                  ),
                                ),
                            itemBuilder: (context, index) {
                              final p = ordered[index];
                              return Padding(
                                key: ValueKey(p.id),
                                padding: const EdgeInsets.only(bottom: 16),
                                child: SizedBox(
                                  height: narrowHeight(p),
                                  child: card(p, wide: false),
                                ),
                              );
                            },
                          ),
                  ),
                ],
                surfaces: [
                  for (final p in widget.panels)
                    Positioned.fill(
                      key: ValueKey(p.id),
                      child: BoardSurfaceHost(
                        anchor: anchor(p.id),
                        resizeEdges: resizeEdges,
                        child: ListenableBuilder(
                          listenable: widget.session,
                          builder: (context, _) {
                            if (!widget.session.surfaceIds.contains(
                              p.reference.id,
                            )) {
                              return const Center(
                                child: Icon(Icons.cloud_off_outlined),
                              );
                            }
                            final surface = widget.session.contextForSurface(
                              p.reference.id,
                            );
                            return ExcludeFocus(
                              excluding: !widget.interact,
                              child: IgnorePointer(
                                ignoring: !widget.interact,
                                child: Surface(
                                  key: ValueKey(surface.surfaceId),
                                  surfaceContext: surface,
                                ),
                              ),
                            );
                          },
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      );
    },
  );
}

class _HandleDragPolicy extends grid.DashboardPolicy {
  const _HandleDragPolicy(this.allowed);
  final bool Function(String) allowed;
  @override
  bool canDrag(grid.LayoutItem item) => allowed(item.id);
}
