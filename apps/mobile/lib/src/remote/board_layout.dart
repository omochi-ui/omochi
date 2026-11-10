import 'board_models.dart';

bool validLayout(List<BoardPanel> panels) {
  if (panels.length > 200 ||
      panels.map((p) => p.id).toSet().length != panels.length) {
    return false;
  }
  for (var i = 0; i < panels.length; i++) {
    final p = panels[i];
    if (p.x < 0 ||
        p.y < 0 ||
        p.y > 100000 ||
        p.w < 1 ||
        p.x + p.w > 12 ||
        p.h < 1 ||
        p.h > 1000) {
      return false;
    }
    for (final q in panels.take(i)) {
      if (p.x < q.x + q.w &&
          q.x < p.x + p.w &&
          p.y < q.y + q.h &&
          q.y < p.y + p.h) {
        return false;
      }
    }
  }
  return true;
}

List<BoardPanel> readingOrder(List<BoardPanel> panels) =>
    [...panels]..sort((a, b) {
      final row = a.y.compareTo(b.y);
      return row == 0 ? a.x.compareTo(b.x) : row;
    });

BoardPanel geometry(BoardPanel p, int x, int y, int w, int h) => BoardPanel(
  id: p.id,
  title: p.title,
  reference: p.reference,
  x: x,
  y: y,
  w: w,
  h: h,
);

/// Pack in explicit user order without compaction that could undo that order.
List<BoardPanel> packPanels(List<BoardPanel> panels) {
  var x = 0, y = 0, height = 0;
  return panels.map((p) {
    if (x + p.w > 12) {
      x = 0;
      y += height;
      height = 0;
    }
    final next = geometry(p, x, y, p.w, p.h);
    x += p.w;
    if (p.h > height) height = p.h;
    return next;
  }).toList();
}
