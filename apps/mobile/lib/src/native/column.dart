import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:genui/genui.dart';

/// Keeps flex allocation finite even when a Column is measured by another
/// Column's scrollable content. Intrinsic sizing cannot measure scroll views.
class _ColumnViewport extends InheritedWidget {
  const _ColumnViewport({required this.height, required super.child});
  final double height;

  @override
  bool updateShouldNotify(_ColumnViewport oldWidget) =>
      height != oldWidget.height;
}

final nativeColumn = CatalogItem(
  name: BasicCatalogItems.column.name,
  dataSchema: BasicCatalogItems.column.dataSchema,
  widgetBuilder: (item) {
    final data = item.data as Map;

    bool needsBoundedHeight(String id) {
      final pending = [id];
      final visited = <String>{};
      while (pending.isNotEmpty) {
        final current = pending.removeLast();
        if (!visited.add(current)) continue;
        final component = item.getComponent(current);
        if (component == null) continue;
        final properties = component.properties;
        switch (component.type) {
          case 'Row':
            if (properties['align'] == 'stretch') return true;
            final children = properties['children'];
            if (children is List) {
              pending.addAll(children.cast<String>());
            } else if (children is Map && children['componentId'] is String) {
              pending.add(children['componentId'] as String);
            }
          case 'Card' || 'Button':
            if (properties['child'] case final String child) pending.add(child);
          case 'Modal':
            if (properties['trigger'] case final String child) {
              pending.add(child);
            }
          // Columns and scroll views establish their own height constraints.
        }
      }
      return false;
    }

    Widget child(String id, DataContext context, {Key? key}) {
      final component = item.getComponent(id);
      final explicit = (component?.properties['weight'] as num?)?.toDouble();
      final implicit =
          item.getCatalogItem(component?.type ?? '')?.isImplicitlyFlexible ??
          false;
      return _ColumnChild(
        key: key,
        weight: math.max(0.0, explicit ?? (implicit ? 1 : 0)),
        tight: explicit != null,
        boundedHeight: needsBoundedHeight(id),
        child: item.buildChild(id, context),
      );
    }

    Widget layout(List<Widget> children, {required double spacing}) =>
        LayoutBuilder(
          builder: (context, constraints) {
            final inherited = context
                .dependOnInheritedWidgetOfExactType<_ColumnViewport>();
            final height = constraints.hasBoundedHeight
                ? constraints.maxHeight
                : inherited?.height ??
                      math.max(
                        0.0,
                        MediaQuery.sizeOf(context).height -
                            MediaQuery.viewInsetsOf(context).bottom -
                            MediaQuery.paddingOf(context).vertical,
                      );
            return _ColumnViewport(
              height: height,
              child: ConstrainedBox(
                constraints: BoxConstraints(maxHeight: height),
                child: SingleChildScrollView(
                  primary: false,
                  child: _ColumnLayout(
                    viewportHeight: height,
                    minHeight: constraints.minHeight,
                    spacing: spacing,
                    justify: data['justify'] as String? ?? 'start',
                    align: data['align'] as String? ?? 'start',
                    textDirection: Directionality.of(context),
                    children: children,
                  ),
                ),
              ),
            );
          },
        );

    return ComponentChildrenBuilder(
      childrenData: data['children'],
      dataContext: item.dataContext,
      buildChild: item.buildChild,
      getComponent: item.getComponent,
      explicitListBuilder: (ids, build, get, context) =>
          layout([for (final id in ids) child(id, context)], spacing: 8),
      templateListWidgetBuilder: (context, values, id, path) {
        final keys = switch (values) {
          List() => List.generate(values.length, (i) => '$i'),
          Map() => values.keys.map((key) => key.toString()).toList(),
          _ => <String>[],
        };
        // GenUI's template Columns have no inter-item spacing.
        return layout([
          for (final key in keys)
            child(
              id,
              item.dataContext.nested(
                // A map key is one JSON Pointer segment, even with / or ~.
                DataPath(
                  '$path/${key.replaceAll('~', '~0').replaceAll('/', '~1')}',
                ),
              ),
              key: ValueKey(key),
            ),
        ], spacing: 0);
      },
    );
  },
);

class _ColumnData extends ContainerBoxParentData<RenderBox> {
  double weight = 0;
  bool tight = false;
  bool boundedHeight = false;
}

class _ColumnChild extends ParentDataWidget<_ColumnData> {
  const _ColumnChild({
    super.key,
    required this.weight,
    required this.tight,
    required this.boundedHeight,
    required super.child,
  });
  final double weight;
  final bool tight, boundedHeight;

  @override
  void applyParentData(RenderObject renderObject) {
    final data = renderObject.parentData! as _ColumnData;
    if (data.weight == weight &&
        data.tight == tight &&
        data.boundedHeight == boundedHeight) {
      return;
    }
    data.weight = weight;
    data.tight = tight;
    data.boundedHeight = boundedHeight;
    renderObject.parent?.markNeedsLayout();
  }

  @override
  Type get debugTypicalAncestorWidgetClass => _ColumnLayout;
}

class _ColumnLayout extends MultiChildRenderObjectWidget {
  const _ColumnLayout({
    required this.viewportHeight,
    required this.minHeight,
    required this.spacing,
    required this.justify,
    required this.align,
    required this.textDirection,
    required super.children,
  });
  final double viewportHeight, minHeight, spacing;
  final String justify, align;
  final TextDirection textDirection;

  @override
  RenderObject createRenderObject(BuildContext context) => _RenderColumn(this);

  @override
  void updateRenderObject(BuildContext context, _RenderColumn renderObject) =>
      renderObject.update(this);
}

class _RenderColumn extends RenderBox
    with
        ContainerRenderObjectMixin<RenderBox, _ColumnData>,
        RenderBoxContainerDefaultsMixin<RenderBox, _ColumnData> {
  _RenderColumn(this.config);
  _ColumnLayout config;

  void update(_ColumnLayout value) {
    config = value;
    markNeedsLayout();
  }

  @override
  void setupParentData(RenderBox child) {
    if (child.parentData is! _ColumnData) child.parentData = _ColumnData();
  }

  @override
  void performLayout() {
    final children = getChildrenAsList();
    final stretch = config.align == 'stretch' && constraints.hasBoundedWidth;
    BoxConstraints childConstraints({
      double min = 0,
      double max = double.infinity,
    }) => BoxConstraints(
      minWidth: stretch ? constraints.maxWidth : 0,
      maxWidth: constraints.maxWidth,
      minHeight: min,
      maxHeight: max,
    );

    var used = config.spacing * math.max(0, children.length - 1);
    var totalWeight = 0.0;
    var width = 0.0;
    // Measure only non-flex children, using real layout rather than intrinsics.
    for (final child in children) {
      final data = child.parentData! as _ColumnData;
      totalWeight += data.weight;
      if (data.weight > 0) continue;
      // A stretch Row passes its maximum height as a tight constraint to
      // its children, including through wrappers. Give it a viewport instead
      // of infinity; other natural children remain unbounded so tall content
      // can scroll.
      child.layout(
        childConstraints(
          max: data.boundedHeight ? config.viewportHeight : double.infinity,
        ),
        parentUsesSize: true,
      );
      used += child.size.height;
      width = math.max(width, child.size.width);
    }
    final remaining = config.viewportHeight - used;
    final budget = remaining > 0 ? remaining : config.viewportHeight;
    for (final child in children) {
      final data = child.parentData! as _ColumnData;
      if (data.weight == 0) continue;
      final allocation = budget * data.weight / totalWeight;
      child.layout(
        childConstraints(min: data.tight ? allocation : 0, max: allocation),
        parentUsesSize: true,
      );
      used += child.size.height;
      width = math.max(width, child.size.width);
    }
    size = constraints.constrain(Size(width, math.max(used, config.minHeight)));
    final free = math.max(0.0, size.height - used);
    final count = children.length;
    final (leading, between) = switch (config.justify) {
      'center' => (free / 2, 0.0),
      'end' => (free, 0.0),
      'spaceBetween' when count > 1 => (0.0, free / (count - 1)),
      'spaceAround' when count > 0 => (free / count / 2, free / count),
      'spaceEvenly' => (free / (count + 1), free / (count + 1)),
      _ => (0.0, 0.0),
    };
    var y = leading;
    for (final child in children) {
      final extra = size.width - child.size.width;
      final x = switch (config.align) {
        'center' => extra / 2,
        'end' => config.textDirection == TextDirection.ltr ? extra : 0.0,
        _ => config.textDirection == TextDirection.rtl ? extra : 0.0,
      };
      (child.parentData! as _ColumnData).offset = Offset(x, y);
      y += child.size.height + config.spacing + between;
    }
  }

  @override
  void paint(PaintingContext context, Offset offset) =>
      defaultPaint(context, offset);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      defaultHitTestChildren(result, position: position);
}
