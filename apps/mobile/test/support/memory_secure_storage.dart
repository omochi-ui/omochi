import 'dart:async';

import 'package:flutter_secure_storage/flutter_secure_storage.dart';

class MemorySecureStorage extends FlutterSecureStorage {
  String? value;
  int writes = 0;
  bool failRead = false, failAfterWrite = false;
  Completer<void>? writeStarted, finishWrite;
  @override
  Future<String?> read({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    if (failRead) throw StateError('storage unavailable');
    return value;
  }

  @override
  Future<void> write({
    required String key,
    required String? value,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    writes++;
    writeStarted?.complete();
    await finishWrite?.future;
    this.value = value;
    if (failAfterWrite) throw StateError('write result unavailable');
  }

  @override
  Future<void> delete({
    required String key,
    AppleOptions? iOptions,
    AndroidOptions? aOptions,
    LinuxOptions? lOptions,
    WebOptions? webOptions,
    AppleOptions? mOptions,
    WindowsOptions? wOptions,
  }) async {
    value = null;
  }
}
