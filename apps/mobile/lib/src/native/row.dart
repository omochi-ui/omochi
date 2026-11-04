import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:genui/genui.dart';

/// Flutter's Flexible accepts integer flex, but A2UI weights are real numbers.
/// Allocate widths directly so fractional weights retain their ratios.
final nativeRow = CatalogItem(
  name: BasicCatalogItems.row.name,
  dataSchema: BasicCatalogItems.row.dataSchema,
  widgetBuilder: (item) {
    final data = item.data as Map;

    Widget child(String id, DataContext context, {Key? key}) {
      final component = item.getComponent(id);
      final explicit = (component?.properties['weight'] as num?)?.toDouble();
      final implicit =
          item.getCatalogItem(component?.type ?? '')?.isImplicitlyFlexible ??
          false;
      return _RowChild(
        key: key,
        weight: math.max(0.0, explicit ?? (implicit ? 1 : 0)),
        tight: explicit != null,
        child: item.buildChild(id, context),
      );
    }

    Widget layout(List<Widget> children, {required double spacing}) =>
        _RowLayout(
          spacing: spacing,
          justify: data['justify'] as String? ?? 'start',
          align: data['align'] as String? ?? 'start',
          textDirection: Directionality.of(item.buildContext),
          children: children,
        );

    return ComponentChildrenBuilder(
      childrenData: data['children'],
      dataContext: item.dataContext,
      buildChild: item.buildChild,
      getComponent: item.getComponent,
      explicitListBuilder: (ids, build, get, context) =>
          layout([for (final id in ids) child(id, context)], spacing: 16),
      templateListWidgetBuilder: (context, values, id, path) {
        final keys = switch (values) {
          List() => List.generate(values.length, (i) => '$i'),
          Map() => values.keys.map((key) => key.toString()).toList(),
          _ => <String>[],
        };
        // Match GenUI's template Rows, which have no inter-item spacing.
        return layout([
          for (final key in keys)
            child(
              id,
              item.dataContext.nested(DataPath('$path/$key')),
              key: ValueKey(key),
            ),
        ], spacing: 0);
      },
    );
  },
);

class _RowData extends ContainerBoxParentData<RenderBox> {
  double weight = 0;
  bool tight = false;
}

class _RowChild extends ParentDataWidget<_RowData> {
  const _RowChild({
    super.key,
    required this.weight,
    required this.tight,
    required super.child,
  });
  final double weight;
  final bool tight;

  @override
  void applyParentData(RenderObject renderObject) {
    final data = renderObject.parentData! as _RowData;
    if (data.weight == weight && data.tight == tight) return;
    data.weight = weight;
    data.tight = tight;
    renderObject.parent?.markNeedsLayout();
  }

  @override
  Type get debugTypicalAncestorWidgetClass => _RowLayout;
}

class _RowLayout extends MultiChildRenderObjectWidget {
  const _RowLayout({
    required this.spacing,
    required this.justify,
    required this.align,
    required this.textDirection,
    required super.children,
  });
  final double spacing;
  final String justify, align;
  final TextDirection textDirection;

  @override
  RenderObject createRenderObject(BuildContext context) => _RenderRow(this);

  @override
  void updateRenderObject(BuildContext context, _RenderRow renderObject) =>
      renderObject.update(this);
}

class _RenderRow extends RenderBox
    with
        ContainerRenderObjectMixin<RenderBox, _RowData>,
        RenderBoxContainerDefaultsMixin<RenderBox, _RowData> {
  _RenderRow(this.config);
  _RowLayout config;

  void update(_RowLayout value) {
    config = value;
    markNeedsLayout();
  }

  @override
  void setupParentData(RenderBox child) {
    if (child.parentData is! _RowData) child.parentData = _RowData();
  }

  @override
  void performLayout() {
    final children = getChildrenAsList();
    final stretch = config.align == 'stretch';
    BoxConstraints childConstraints({
      double min = 0,
      double max = double.infinity,
    }) => BoxConstraints(
      minWidth: min,
      maxWidth: max,
      minHeight: stretch ? constraints.maxHeight : 0,
      maxHeight: constraints.maxHeight,
    );

    var used = config.spacing * math.max(0, children.length - 1);
    var totalWeight = 0.0;
    var height = 0.0;
    for (final child in children) {
      final data = child.parentData! as _RowData;
      // As with a shrink-wrapped Flutter Row, loose flex children may use
      // natural width when the parent does not bound the horizontal axis.
      if (data.weight > 0 && constraints.hasBoundedWidth) {
        totalWeight += data.weight;
        continue;
      }
      assert(
        data.weight == 0 || !data.tight,
        'A Row with tight weighted children needs a finite width.',
      );
      child.layout(childConstraints(), parentUsesSize: true);
      used += child.size.width;
      height = math.max(height, child.size.height);
    }
    final budget = math.max(0.0, constraints.maxWidth - used);
    if (totalWeight > 0) {
      for (final child in children) {
        final data = child.parentData! as _RowData;
        if (data.weight == 0) continue;
        final allocation = budget * (data.weight / totalWeight);
        child.layout(
          childConstraints(min: data.tight ? allocation : 0, max: allocation),
          parentUsesSize: true,
        );
        used += child.size.width;
        height = math.max(height, child.size.height);
      }
    }
    size = constraints.constrain(Size(used, height));
    final free = math.max(0.0, size.width - used);
    final count = children.length;
    final (leading, between) = switch (config.justify) {
      'center' => (free / 2, 0.0),
      'end' => (free, 0.0),
      'spaceBetween' when count > 1 => (0.0, free / (count - 1)),
      'spaceAround' when count > 0 => (free / count / 2, free / count),
      'spaceEvenly' => (free / (count + 1), free / (count + 1)),
      _ => (0.0, 0.0),
    };
    final rtl = config.textDirection == TextDirection.rtl;
    var x = leading;
    for (final child in children) {
      final extra = size.height - child.size.height;
      final y = switch (config.align) {
        'center' => extra / 2,
        'end' => extra,
        _ => 0.0,
      };
      (child.parentData! as _RowData).offset = Offset(
        rtl ? size.width - x - child.size.width : x,
        y,
      );
      x += child.size.width + config.spacing + between;
    }
  }

  @override
  void paint(PaintingContext context, Offset offset) =>
      defaultPaint(context, offset);

  @override
  bool hitTestChildren(BoxHitTestResult result, {required Offset position}) =>
      defaultHitTestChildren(result, position: position);
}
