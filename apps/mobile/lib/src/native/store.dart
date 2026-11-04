import 'dart:async';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Identity and registered peers use OS-protected storage.
/// Mutations re-read under the app serial lock.
class NativeHubStore {
  NativeHubStore({
    this.namespace = 'omochi.mobile.v1',
    FlutterSecureStorage? secure,
  }) : secure =
           secure ??
           const FlutterSecureStorage(
             aOptions: AndroidOptions(resetOnError: false),
             // The app does not share Keychain items. This keeps macOS builds
             // usable with ad-hoc signing without a provisioning profile.
             mOptions: MacOsOptions(usesDataProtectionKeychain: false),
           );
  final String namespace;
  final FlutterSecureStorage secure;
  static Future<void> _writes = Future<void>.value();
  Future<T> _locked<T>(Future<T> Function() operation) {
    final result = _writes.then((_) => operation());
    _writes = result.then<void>((_) {}, onError: (Object _, StackTrace _) {});
    return result;
  }

  static final Set<String> _active = {};

  /// Prevent two clients from owning the same document together.
  /// CAS below still protects against stale data from another storage writer.
  void Function() acquireIdentityLease() {
    if (!_active.add(namespace)) {
      throw StateError('This Hub is already running');
    }
    var released = false;
    return () {
      if (!released) {
        released = true;
        _active.remove(namespace);
      }
    };
  }

  Future<String?> readIdentityDocument() =>
      secure.read(key: '$namespace.identity');

  Future<String> commitIdentityDocument(
    String? expected,
    String replacement,
  ) => _locked(() async {
    final String? current;
    try {
      current = await readIdentityDocument();
    } catch (_) {
      return 'failed';
    }
    if (current != expected) return 'conflict';
    try {
      await secure.write(key: '$namespace.identity', value: replacement);
      return 'committed';
    } catch (_) {
      // Platform exceptions do not prove that the secure write had no effect.
      return 'unknown';
    }
  });

  Future<bool> online() async =>
      (await SharedPreferencesAsync().getBool('$namespace.online')) ?? true;
  Future<void> setOnline(bool value) =>
      SharedPreferencesAsync().setBool('$namespace.online', value);
  Future<void> clearTestNamespace() async {
    if (!namespace.startsWith('omochi.test.')) {
      throw StateError('Not a test store');
    }
    await secure.delete(key: '$namespace.identity');
    await SharedPreferencesAsync().remove('$namespace.online');
  }
}
