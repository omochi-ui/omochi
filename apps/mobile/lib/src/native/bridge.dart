import 'dart:async';
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:ffi/ffi.dart';

import '../models.dart';
import 'store.dart';

class BridgeCall {
  BridgeCall(this.result, this.cancel);
  final Future<dynamic> result;
  final void Function() cancel;
}

/// Commands return immediately. Polling services secure-storage callbacks even
/// while a mutation or close is waiting; no synchronous call waits for Dart.
class SharedHubBridge {
  SharedHubBridge(this.store, {DynamicLibrary? library}) {
    final lib =
        library ??
        (Platform.isIOS
            ? DynamicLibrary.process()
            : DynamicLibrary.open(
                Platform.isWindows
                    ? File(Platform.resolvedExecutable).parent.uri
                          .resolve('omochi_native_bridge.dll')
                          .toFilePath()
                    : Platform.isMacOS
                    ? File(Platform.resolvedExecutable).parent.parent.uri
                          .resolve('Frameworks/libomochi_native_bridge.dylib')
                          .toFilePath()
                    : Platform.isLinux
                    ? File(Platform.resolvedExecutable).parent.uri
                          .resolve('lib/libomochi_native_bridge.so')
                          .toFilePath()
                    : 'libomochi_native_bridge.so',
              ));
    final version = lib.lookupFunction<Uint32 Function(), int Function()>(
      'omochi_hub_abi_version',
    );
    if (version() != 2) throw StateError('Unsupported shared Hub binding');
    _create = lib.lookupFunction<Uint64 Function(), int Function()>(
      'omochi_hub_create',
    );
    _submit = lib
        .lookupFunction<
          Uint64 Function(Uint64, Pointer<Uint8>, Size),
          int Function(int, Pointer<Uint8>, int)
        >('omochi_hub_submit');
    _poll = lib
        .lookupFunction<
          Pointer<Utf8> Function(Uint64),
          Pointer<Utf8> Function(int)
        >('omochi_hub_poll');
    _free = lib
        .lookupFunction<
          Void Function(Pointer<Utf8>),
          void Function(Pointer<Utf8>)
        >('omochi_hub_string_free');
    _reply = lib
        .lookupFunction<
          Bool Function(Uint64, Uint64, Pointer<Uint8>, Size),
          bool Function(int, int, Pointer<Uint8>, int)
        >('omochi_hub_storage_reply');
    _cancel = lib
        .lookupFunction<Void Function(Uint64, Uint64), void Function(int, int)>(
          'omochi_hub_cancel',
        );
    _read = lib
        .lookupFunction<
          IntPtr Function(Uint64, Uint64, Size, Pointer<Uint8>, Size),
          int Function(int, int, int, Pointer<Uint8>, int)
        >('omochi_hub_blob_read');
    _freeBlob = lib
        .lookupFunction<Void Function(Uint64, Uint64), void Function(int, int)>(
          'omochi_hub_blob_free',
        );
    _release = lib.lookupFunction<Bool Function(Uint64), bool Function(int)>(
      'omochi_hub_release',
    );
    _id = _create();
    if (_id == 0) throw StateError('Too many shared Hub instances');
    _timer = Timer.periodic(const Duration(milliseconds: 8), (_) => _drain());
  }
  final NativeHubStore store;
  late final int Function() _create;
  late final int Function(int, Pointer<Uint8>, int) _submit;
  late final Pointer<Utf8> Function(int) _poll;
  late final void Function(Pointer<Utf8>) _free;
  late final bool Function(int, int, Pointer<Uint8>, int) _reply;
  late final void Function(int, int) _cancel;
  late final int Function(int, int, int, Pointer<Uint8>, int) _read;
  late final void Function(int, int) _freeBlob;
  late final bool Function(int) _release;
  late final int _id;
  late final Timer _timer;
  final Map<int, Completer<dynamic>> _pending = {};
  bool _closed = false;
  Future<void>? _closing;

  T _encoded<T>(Json value, T Function(Pointer<Uint8>, int) call) {
    final bytes = utf8.encode(jsonEncode(value));
    if (bytes.length > 4 * 1024 * 1024) {
      throw StateError('Hub command exceeds capacity');
    }
    final pointer = calloc<Uint8>(bytes.length);
    try {
      pointer.asTypedList(bytes.length).setAll(0, bytes);
      return call(pointer, bytes.length);
    } finally {
      calloc.free(pointer);
    }
  }

  BridgeCall call(Json command) {
    if (_closed) throw StateError('Hub stopped');
    final id = _encoded(command, (p, n) => _submit(_id, p, n));
    if (id == 0) {
      throw StateError('Hub is busy or stopping; request was not sent');
    }
    final result = Completer<dynamic>();
    _pending[id] = result;
    return BridgeCall(result.future, () => _cancel(_id, id));
  }

  Future<dynamic> request(Json command) => call(command).result;

  void _drain() {
    // Bound UI work per tick, including result JSON parsing. Rust retains batch
    // credit until the applied ACK; this does not prefetch another render batch.
    for (var i = 0; i < 4; i++) {
      final pointer = _poll(_id);
      if (pointer == nullptr) break;
      final Json value;
      try {
        value = jsonDecode(pointer.toDartString()) as Json;
      } finally {
        _free(pointer);
      }
      switch (value['kind']) {
        case 'storage':
          unawaited(_storage(value));
        case 'result':
          final result = _pending.remove(value['id']);
          if (result == null) continue;
          if (value['ok'] == true) {
            result.complete(value['value']);
          } else {
            result.completeError(StateError(value['error'] as String));
          }
        case 'terminal':
          final error = StateError(value['error'] as String);
          for (final result in _pending.values) {
            result.completeError(error);
          }
          _pending.clear();
      }
    }
  }

  Future<void> _storage(Json value) async {
    Json response;
    if (value['op'] == 'read') {
      try {
        response = {'ok': true, 'value': await store.readIdentityDocument()};
      } catch (_) {
        response = {'ok': false};
      }
    } else {
      try {
        response = {
          'outcome': await store.commitIdentityDocument(
            value['expected'] as String?,
            value['replacement'] as String,
          ),
        };
      } catch (_) {
        response = {'outcome': 'unknown'};
      }
    }
    _encoded(response, (p, n) => _reply(_id, value['id'] as int, p, n));
  }

  Future<Uint8List> imageBytes(Json value, bool Function() cancelled) async {
    final blob = value['blob'] as int;
    final length = value['length'] as int;
    Pointer<Uint8>? buffer;
    try {
      if (length < 1 || length > 8 * 1024 * 1024) {
        throw StateError('Image exceeds capacity');
      }
      final bytes = Uint8List(length);
      buffer = calloc<Uint8>(64 * 1024);
      for (var offset = 0; offset < length;) {
        if (cancelled()) throw StateError('Image request cancelled');
        final size = _read(_id, blob, offset, buffer, 64 * 1024);
        if (size <= 0 || offset + size > length) {
          throw StateError('Image handle expired');
        }
        bytes.setRange(offset, offset + size, buffer.asTypedList(size));
        offset += size;
        if (offset % (256 * 1024) == 0) {
          await Future<void>.delayed(Duration.zero);
        }
      }
      if (cancelled()) throw StateError('Image request cancelled');
      return bytes;
    } finally {
      if (buffer != null) calloc.free(buffer);
      _freeBlob(_id, blob);
    }
  }

  Future<void> close() => _closing ??= _close().catchError((Object e) {
    _closing = null;
    throw e;
  });
  Future<void> _close() async {
    if (_closed) return;
    await request({'op': 'close'});
    final deadline = DateTime.now().add(const Duration(seconds: 15));
    while (!_release(_id)) {
      if (DateTime.now().isAfter(deadline)) {
        throw StateError('Hub shutdown has not completed');
      }
      await Future<void>.delayed(const Duration(milliseconds: 8));
    }
    _closed = true;
    _timer.cancel();
    for (final result in _pending.values) {
      result.completeError(StateError('Hub stopped'));
    }
    _pending.clear();
  }
}
