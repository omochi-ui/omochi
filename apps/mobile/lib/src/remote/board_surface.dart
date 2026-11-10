import 'package:flutter/rendering.dart';
import 'package:flutter/material.dart';

/// A stable surface host follows the body slot of a disposable sliver card.
/// Keeping the host outside the slivers preserves input state when a slot is
/// offscreen, replaced at a breakpoint, or lifted into drag feedback.
class BoardSurfaceAnchor {
  final link = LayerLink();
  // The projected body position also lets ensureVisible reach a visited panel
  // after its sliver slot has been collected, including after width changes.
  Offset contentOrigin = Offset.zero;
  _RenderSurfaceSlot? _slot;
  final size = ValueNotifier<Size?>(null);
  final visible = ValueNotifier(false);
  Size? _pendingSize;
  bool _scheduled = false;
  bool _disposed = false;

  void didLayout(Size value) {
    _pendingSize = value;
    refresh();
  }

  void refresh() {
    if (_disposed || _scheduled) return;
    _scheduled = true;
    // Lazy slivers can lay out independently of the stable hosts. Publish
    // after the frame so their layout never rebuilds another subtree.
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _scheduled = false;
      if (!_disposed) {
        size.value = _pendingSize;
        visible.value = link.leader != null;
      }
    });
  }

  void dispose() {
    _disposed = true;
    size.dispose();
    visible.dispose();
  }
}

class BoardSurfaceSlot extends SingleChildRenderObjectWidget {
  const BoardSurfaceSlot({super.key, required this.anchor})
    : super(child: const SizedBox.expand());
  final BoardSurfaceAnchor anchor;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderSurfaceSlot(anchor);
}

class _RenderSurfaceSlot extends RenderLeaderLayer {
  _RenderSurfaceSlot(this.anchor) : super(link: anchor.link);
  final BoardSurfaceAnchor anchor;

  @override
  void attach(PipelineOwner owner) {
    super.attach(owner);
    anchor._slot = this;
  }

  @override
  void detach() {
    if (identical(anchor._slot, this)) anchor._slot = null;
    super.detach();
  }

  @override
  void performLayout() {
    super.performLayout();
    anchor.didLayout(size);
  }
}

class BoardSurfaceHost extends StatelessWidget {
  const BoardSurfaceHost({
    super.key,
    required this.anchor,
    required this.child,
    required this.resizeEdges,
  });
  final BoardSurfaceAnchor anchor;
  final Widget child;
  final bool resizeEdges;

  @override
  Widget build(BuildContext context) => Semantics(
    tagForChildren: RenderViewport.useTwoPaneSemantics,
    child: ValueListenableBuilder(
      valueListenable: anchor.size,
      child: child,
      builder: (context, size, child) => size == null
          ? const SizedBox.shrink()
          : ValueListenableBuilder(
              valueListenable: anchor.visible,
              builder: (context, visible, child) => ExcludeFocus(
                excluding: !visible,
                child: TickerMode(
                  enabled: visible,
                  child: Offstage(offstage: !visible, child: child),
                ),
              ),
              // Keep the follower's layout and hit bounds at the full slot
              // size, even when the body is taller than the scroll viewport.
              child: OverflowBox(
                alignment: Alignment.topLeft,
                minWidth: size.width,
                maxWidth: size.width,
                minHeight: size.height,
                maxHeight: size.height,
                child: CompositedTransformFollower(
                  link: anchor.link,
                  showWhenUnlinked: false,
                  child: _SurfaceBody(
                    anchor: anchor,
                    resizeEdges: resizeEdges,
                    child: ClipRRect(
                      borderRadius: const BorderRadius.vertical(
                        bottom: Radius.circular(12),
                      ),
                      child: Material(
                        type: MaterialType.transparency,
                        child: child,
                      ),
                    ),
                  ),
                ),
              ),
            ),
    ),
  );
}

// The dashboard package resolves resize gestures by hit testing its sliver.
// When native resizing is active, pass its 20px edge targets through to the
// card under the surface. Android/iOS app content keeps its entire hit area.
class _SurfaceBody extends SingleChildRenderObjectWidget {
  const _SurfaceBody({
    required this.anchor,
    required this.resizeEdges,
    required super.child,
  });
  final BoardSurfaceAnchor anchor;
  final bool resizeEdges;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderSurfaceBody(anchor, resizeEdges);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderSurfaceBody renderObject,
  ) {
    renderObject.resizeEdges = resizeEdges;
  }
}

class _RenderSurfaceBody extends RenderProxyBox {
  _RenderSurfaceBody(this.anchor, this.resizeEdges);
  final BoardSurfaceAnchor anchor;
  bool resizeEdges;

  @override
  bool hitTest(BoxHitTestResult result, {required Offset position}) {
    if (resizeEdges &&
        (position.dx < 20 ||
            position.dx > size.width - 20 ||
            position.dy > size.height - 20)) {
      return false;
    }
    return super.hitTest(result, position: position);
  }
}

/// Surface hosts share the Scrollable ancestor with the sliver cards, so wheel
/// and touch scrolling over application content still scrolls the dashboard.
class BoardScrollView extends CustomScrollView {
  const BoardScrollView({
    super.key,
    required super.controller,
    required super.slivers,
    required this.surfaces,
  });
  final List<Widget> surfaces;

  @override
  Widget buildViewport(
    BuildContext context,
    ViewportOffset offset,
    AxisDirection axisDirection,
    List<Widget> slivers,
  ) => _SurfaceViewport(
    offset: offset,
    child: ClipRect(
      child: Stack(
        fit: StackFit.expand,
        children: [
          super.buildViewport(context, offset, axisDirection, slivers),
          ...surfaces,
        ],
      ),
    ),
  );
}

// The stable hosts are siblings of RenderViewport. Give their descendants the
// same reveal contract as normal sliver children for caret and ensureVisible.
class _SurfaceViewport extends SingleChildRenderObjectWidget {
  const _SurfaceViewport({required this.offset, required super.child});
  final ViewportOffset offset;

  @override
  RenderObject createRenderObject(BuildContext context) =>
      _RenderSurfaceViewport(offset);

  @override
  void updateRenderObject(
    BuildContext context,
    _RenderSurfaceViewport renderObject,
  ) {
    renderObject.offset = offset;
  }
}

class _RenderSurfaceViewport extends RenderProxyBox
    implements RenderAbstractViewport {
  _RenderSurfaceViewport(this.offset);
  ViewportOffset offset;

  @override
  RevealedOffset getOffsetToReveal(
    RenderObject target,
    double alignment, {
    Rect? rect,
    Axis? axis,
  }) {
    // Follower transforms reflect the last composition and are unavailable
    // when unlinked. Use the live slot, or its current layout projection.
    RenderObject? body = target;
    while (body != null && body != this && body is! _RenderSurfaceBody) {
      body = body.parent;
    }
    final Rect bounds;
    if (body is _RenderSurfaceBody) {
      final slot = body.anchor._slot;
      final origin = slot != null && slot.attached
          ? MatrixUtils.transformPoint(slot.getTransformTo(this), Offset.zero)
          : body.anchor.contentOrigin - Offset(0, offset.pixels);
      bounds = MatrixUtils.transformRect(
        target.getTransformTo(body),
        rect ?? target.paintBounds,
      ).shift(origin);
    } else {
      bounds = MatrixUtils.transformRect(
        target.getTransformTo(this),
        rect ?? target.paintBounds,
      );
    }
    final delta = bounds.top - (size.height - bounds.height) * alignment;
    return RevealedOffset(
      offset: offset.pixels + delta,
      rect: bounds.shift(Offset(0, -delta)),
    );
  }

  @override
  void showOnScreen({
    RenderObject? descendant,
    Rect? rect,
    Duration duration = Duration.zero,
    Curve curve = Curves.ease,
  }) {
    if (!offset.allowImplicitScrolling) {
      return super.showOnScreen(
        descendant: descendant,
        rect: rect,
        duration: duration,
        curve: curve,
      );
    }
    final revealed = RenderViewportBase.showInViewport(
      descendant: descendant,
      viewport: this,
      offset: offset,
      rect: rect,
      duration: duration,
      curve: curve,
    );
    super.showOnScreen(rect: revealed, duration: duration, curve: curve);
  }
}
