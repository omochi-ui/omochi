import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:genui/genui.dart';
import 'package:sliver_dashboard/sliver_dashboard.dart' as grid;

import '../hub_session.dart';
import '../messages.dart';
import 'board_models.dart';
import 'board_layout.dart';

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
  final keys = <String, GlobalKey>{};
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
    sync();
  }

  void sync() {
    importing = true;
    controller.cancelInteraction();
    controller.setCompactionType(grid.CompactType.none);
    controller.importLayout([for (final i in layout) i.toMap()]);
    controller.setCompactionType(
      editing ? grid.CompactType.vertical : grid.CompactType.none,
    );
    // Strategy changes compact immediately; imports must preserve saved geometry.
    controller.layout.value = layout;
    controller.setEditMode(editing);
    importing = false;
  }

  @override
  void didUpdateWidget(BoardCanvas oldWidget) {
    super.didUpdateWidget(oldWidget);
    sync();
    redraw.value++;
    keys.removeWhere((id, _) => !widget.panels.any((p) => p.id == id));
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    redraw.dispose();
    controller.dispose();
    super.dispose();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) controller.cancelInteraction();
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

  Widget card(BoardPanel panel, Widget? handle) => _RetainedCard(
    key: keys.putIfAbsent(panel.id, GlobalKey.new),
    child: ValueListenableBuilder(
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
                  height: bounds.maxHeight < 96 ? bounds.maxHeight / 2 : 48,
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
                              if (handle != null)
                                handle is grid.DashboardDragStartListener
                                    ? handle
                                    : narrowHandle(
                                        index: index,
                                        child: Semantics(
                                          label: messagesFor(context).boardMove,
                                          child: const Padding(
                                            padding: EdgeInsets.all(12),
                                            child: Icon(Icons.drag_handle),
                                          ),
                                        ),
                                      ),
                              IconButton(
                                tooltip: messagesFor(context).boardUp,
                                onPressed: index > 0
                                    ? () => reorder(index, index - 1)
                                    : null,
                                icon: const Icon(Icons.arrow_upward),
                              ),
                              IconButton(
                                tooltip: messagesFor(context).boardDown,
                                onPressed:
                                    index >= 0 && index < order.length - 1
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
                Expanded(
                  child: ListenableBuilder(
                    listenable: widget.session,
                    builder: (context, _) {
                      if (!widget.session.surfaceIds.contains(p.reference.id)) {
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
              ],
            ),
          ),
        );
      },
    ),
  );

  @override
  Widget build(BuildContext context) => LayoutBuilder(
    builder: (context, c) {
      if (previousWidth != c.maxWidth) {
        controller.cancelInteraction();
        previousWidth = c.maxWidth;
      }
      final ordered = readingOrder(widget.panels);
      return CallbackShortcuts(
        bindings: {
          const SingleActivator(LogicalKeyboardKey.escape):
              controller.cancelInteraction,
        },
        child: c.maxWidth <= 760
            ? CustomScrollView(
                slivers: [
                  SliverPadding(
                    padding: const EdgeInsets.all(16),
                    sliver: SliverReorderableList(
                      itemCount: ordered.length,
                      onReorderItem: reorder,
                      itemBuilder: (context, index) {
                        final p = ordered[index];
                        return Padding(
                          key: ValueKey(p.id),
                          padding: const EdgeInsets.only(bottom: 16),
                          child: SizedBox(
                            height: (p.h * 56 - 16).clamp(320, 600).toDouble(),
                            child: card(
                              p,
                              editing
                                  ? narrowHandle(
                                      index: index,
                                      child: Semantics(
                                        label: messagesFor(context).boardMove,
                                        child: const Padding(
                                          padding: EdgeInsets.all(12),
                                          child: Icon(Icons.drag_handle),
                                        ),
                                      ),
                                    )
                                  : null,
                            ),
                          ),
                        );
                      },
                    ),
                  ),
                ],
              )
            : grid.Dashboard(
                controller: controller,
                padding: const EdgeInsets.all(16),
                slotAspectRatio: ((c.maxWidth - 32 - 11 * 16) / 12) / 40,
                mainAxisSpacing: 16,
                crossAxisSpacing: 16,
                dragStartGesture: grid.DragStartGesture.none,
                itemFeedbackBuilder: (context, item, child) => Material(
                  child: Padding(
                    padding: const EdgeInsets.all(16),
                    child: Text(
                      widget.panels.firstWhere((p) => p.id == item.id).title,
                    ),
                  ),
                ),
                itemBuilder: (context, item) {
                  final p = widget.panels.firstWhere((p) => p.id == item.id);
                  return card(
                    p,
                    editing
                        ? grid.DashboardDragStartListener(
                            itemId: p.id,
                            child: Tooltip(
                              message: messagesFor(context).boardMove,
                              child: const Padding(
                                padding: EdgeInsets.all(8),
                                child: Icon(Icons.drag_handle),
                              ),
                            ),
                          )
                        : null,
                  );
                },
              ),
      );
    },
  );
}

class _RetainedCard extends StatefulWidget {
  const _RetainedCard({super.key, required this.child});
  final Widget child;
  @override
  State<_RetainedCard> createState() => _RetainedCardState();
}

class _RetainedCardState extends State<_RetainedCard>
    with AutomaticKeepAliveClientMixin {
  @override
  bool get wantKeepAlive => true;
  @override
  Widget build(BuildContext context) {
    super.build(context);
    return widget.child;
  }
}
