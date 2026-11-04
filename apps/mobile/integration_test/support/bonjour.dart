import 'dart:async';
import 'dart:ffi';

import 'package:ffi/ffi.dart';

typedef _BrowseReply = Void Function(
  Pointer<Void>,
  Uint32,
  Uint32,
  Int32,
  Pointer<Utf8>,
  Pointer<Utf8>,
  Pointer<Utf8>,
  Pointer<Void>,
);
typedef _ResolveReply = Void Function(
  Pointer<Void>,
  Uint32,
  Uint32,
  Int32,
  Pointer<Utf8>,
  Pointer<Utf8>,
  Uint16,
  Uint16,
  Pointer<Uint8>,
  Pointer<Void>,
);

final class _PollFd extends Struct {
  @Int32()
  external int fd;
  @Int16()
  external int events;
  @Int16()
  external int revents;
}

typedef BonjourRecord = ({
  String name,
  String host,
  int port,
  int interfaceIndex,
});

/// Test the OS DNS-SD API inside the signed, sandboxed Flutter application.
/// Callbacks run synchronously on this isolate during DNSServiceProcessResult.
Future<BonjourRecord> resolveBonjour(String instance) async {
  final library = DynamicLibrary.open('/usr/lib/libSystem.B.dylib');
  final system = DynamicLibrary.process();
  final browse = library
      .lookupFunction<
        Int32 Function(
          Pointer<Pointer<Void>>,
          Uint32,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<NativeFunction<_BrowseReply>>,
          Pointer<Void>,
        ),
        int Function(
          Pointer<Pointer<Void>>,
          int,
          int,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<NativeFunction<_BrowseReply>>,
          Pointer<Void>,
        )
      >('DNSServiceBrowse');
  final resolve = library
      .lookupFunction<
        Int32 Function(
          Pointer<Pointer<Void>>,
          Uint32,
          Uint32,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<NativeFunction<_ResolveReply>>,
          Pointer<Void>,
        ),
        int Function(
          Pointer<Pointer<Void>>,
          int,
          int,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<Utf8>,
          Pointer<NativeFunction<_ResolveReply>>,
          Pointer<Void>,
        )
      >('DNSServiceResolve');
  final socket = library
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('DNSServiceRefSockFD');
  final process = library
      .lookupFunction<
        Int32 Function(Pointer<Void>),
        int Function(Pointer<Void>)
      >('DNSServiceProcessResult');
  final deallocate = library
      .lookupFunction<
        Void Function(Pointer<Void>),
        void Function(Pointer<Void>)
      >('DNSServiceRefDeallocate');
  final poll = system
      .lookupFunction<
        Int32 Function(Pointer<_PollFd>, Uint32, Int32),
        int Function(Pointer<_PollFd>, int, int)
      >('poll');
  final result = Completer<BonjourRecord>();
  final references = <Pointer<Void>>[];
  final output = calloc<Pointer<Void>>();
  final fd = calloc<_PollFd>();
  final type = '_irohv1._udp'.toNativeUtf8();
  final domain = 'local.'.toNativeUtf8();
  var resolving = false;
  Timer? timer;
  void check(int error) {
    if (error != 0 && !result.isCompleted) {
      result.completeError(StateError('DNS-SD error $error'));
    }
  }

  final resolved = NativeCallable<_ResolveReply>.isolateLocal((
    Pointer<Void> ref,
    int flags,
    int index,
    int error,
    Pointer<Utf8> fullname,
    Pointer<Utf8> host,
    int port,
    int txtLength,
    Pointer<Uint8> txt,
    Pointer<Void> context,
  ) {
    check(error);
    if (error != 0 || result.isCompleted) return;
    result.complete((
      name: fullname.toDartString(),
      host: host.toDartString(),
      port: ((port & 255) << 8) | (port >> 8),
      interfaceIndex: index,
    ));
  });
  final found = NativeCallable<_BrowseReply>.isolateLocal((
    Pointer<Void> ref,
    int flags,
    int index,
    int error,
    Pointer<Utf8> name,
    Pointer<Utf8> regtype,
    Pointer<Utf8> replyDomain,
    Pointer<Void> context,
  ) {
    check(error);
    if (error != 0 ||
        resolving ||
        result.isCompleted ||
        flags & 2 == 0 ||
        name.toDartString() != instance) {
      return;
    }
    resolving = true;
    final status = resolve(
      output,
      0,
      index,
      name,
      regtype,
      replyDomain,
      resolved.nativeFunction,
      nullptr,
    );
    if (status == 0) references.add(output.value);
    check(status);
  });
  try {
    final status = browse(
      output,
      0,
      0,
      type,
      domain,
      found.nativeFunction,
      nullptr,
    );
    if (status == 0) references.add(output.value);
    check(status);
    timer = Timer.periodic(const Duration(milliseconds: 20), (_) {
      for (final ref in references.toList()) {
        if (result.isCompleted) break;
        fd.ref
          ..fd = socket(ref)
          ..events = 1
          ..revents = 0;
        final ready = poll(fd, 1, 0);
        if (ready < 0) {
          check(-1);
        } else if (ready > 0 && fd.ref.revents & 1 != 0) {
          check(process(ref));
        }
      }
    });
    return await result.future.timeout(const Duration(seconds: 20));
  } finally {
    timer?.cancel();
    for (final ref in references) {
      deallocate(ref);
    }
    found.close();
    resolved.close();
    calloc.free(output);
    calloc.free(fd);
    calloc.free(type);
    calloc.free(domain);
  }
}

String bonjourInstance(String endpointId) {
  const alphabet = 'abcdefghijklmnopqrstuvwxyz234567';
  final output = StringBuffer();
  var value = 0, bits = 0;
  for (var offset = 0; offset < endpointId.length; offset += 2) {
    value =
        (value << 8) |
        int.parse(endpointId.substring(offset, offset + 2), radix: 16);
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      output.write(alphabet[(value >> bits) & 31]);
    }
    value &= (1 << bits) - 1;
  }
  if (bits > 0) output.write(alphabet[(value << (5 - bits)) & 31]);
  return output.toString();
}
