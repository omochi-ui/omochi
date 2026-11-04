import 'dart:math' as math;

/// Ratios for nonnegative finite weights, without overflowing their sum.
class RelativeWeights {
  RelativeWeights(Iterable<double> weights) {
    _maximum = weights.fold(0.0, math.max);
    _total = _maximum > 0
        ? weights.fold(0.0, (sum, weight) => sum + weight / _maximum)
        : 0;
  }

  late final double _maximum, _total;

  bool get hasWeight => _maximum > 0;

  // Divide before multiplying by a layout budget, which can overflow even
  // when both the budget and the original weight are finite.
  double fraction(double weight) =>
      weight > 0 ? (weight / _maximum) / _total : 0;
}
