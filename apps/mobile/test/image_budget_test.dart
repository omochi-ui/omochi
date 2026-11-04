import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/image_budget.dart';

void main() {
  test(
    'image pixel budget covers all mounted frames and cannot be released twice',
    () {
      final budget = NativeImageBudget();
      final first = budget.reservePixels(2048, 2048);
      final second = budget.reservePixels(2048, 2048);
      expect(budget.decodedBytes, 32 * 1024 * 1024);
      expect(() => budget.reservePixels(1, 1), throwsStateError);
      first();
      first();
      expect(budget.decodedBytes, 16 * 1024 * 1024);
      final third = budget.reservePixels(2048, 2048);
      second();
      third();
      expect(budget.decodedBytes, 0);
      expect(() => budget.reservePixels(8193, 1), throwsStateError);
      expect(() => budget.reservePixels(4096, 4096), throwsStateError);
      expect(() => budget.reservePixels(-1, 1), throwsStateError);
    },
  );
  test('download and decode concurrency stays bounded through cancellation cleanup', () {
    final budget = NativeImageBudget();
    final first = budget.acquireLoad(), second = budget.acquireLoad();
    expect(() => budget.acquireLoad(), throwsStateError);
    first();
    first();
    final third = budget.acquireLoad();
    expect(budget.loading, 2);
    second();
    third();
    expect(budget.loading, 0);
  });
}
