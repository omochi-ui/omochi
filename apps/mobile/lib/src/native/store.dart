import 'dart:async';
import 'dart:io';

import 'package:flutter/services.dart';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:shared_preferences/shared_preferences.dart';

/// Identity and registered peers use OS-protected storage.
/// Mutations re-read under the app serial lock while the Hub owns its lease.
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

  static const _leaseChannel = MethodChannel('omochi/identity-lease');

  /// Held until the native Hub has completely closed. macOS also excludes
  /// other processes; the local guard excludes concurrent callers immediately.
  Future<Future<void> Function()> acquireIdentityLease() async {
    if (!_active.add(namespace)) {
      throw StateError('This Hub is already running');
    }
    String? token;
    try {
      if (Platform.isMacOS) {
        token = await _leaseChannel.invokeMethod<String>('acquire', namespace);
        if (token == null) throw StateError('Identity lease was not acquired');
      }
    } catch (_) {
      _active.remove(namespace);
      rethrow;
    }
    var released = false;
    return () async {
      if (released) return;
      if (token != null) {
        await _leaseChannel.invokeMethod<void>('release', token);
      }
      released = true;
      _active.remove(namespace);
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
