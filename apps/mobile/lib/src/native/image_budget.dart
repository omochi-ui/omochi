import 'dart:ui' as ui;

/// Limits verified with eight 1024×1024 images on Android x86_64.
/// These bound retained pixels, not the whole process RSS.
/// No service image participates in Flutter's process-wide ImageCache.
class NativeImageBudget {
  static const maxEncodedBytes = 8 * 1024 * 1024;
  static const maxPixels = 4 * 1024 * 1024;
  static const maxDimension = 8192;
  static const maxDecodedBytes = 32 * 1024 * 1024;
  static const maxLoading = 2;
  static final shared = NativeImageBudget();
  int _loading = 0, _decoded = 0;
  int get loading => _loading;
  int get decodedBytes => _decoded;

  void Function() acquireLoad() {
    if (_loading >= maxLoading) {
      throw StateError('Image reader is busy; retry later');
    }
    _loading++;
    var active = true;
    return () {
      if (active) {
        active = false;
        _loading--;
      }
    };
  }

  void Function() reservePixels(int width, int height) {
    final pixels = width * height;
    if (width < 1 ||
        height < 1 ||
        width > maxDimension ||
        height > maxDimension ||
        pixels > maxPixels ||
        _decoded + pixels * 4 > maxDecodedBytes) {
      throw StateError('Image exceeds display capacity');
    }
    _decoded += pixels * 4;
    var active = true;
    return () {
      if (active) {
        active = false;
        _decoded -= pixels * 4;
      }
    };
  }
}

class NativeDecodedImage {
  NativeDecodedImage(this.image, this._release);
  final ui.Image image;
  final void Function() _release;
  bool _disposed = false;
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    image.dispose();
    _release();
  }
}
