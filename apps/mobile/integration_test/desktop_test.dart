import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/native/store.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  testWidgets('bundled desktop Hub persists its identity in OS storage', (
    tester,
  ) async {
    final namespace =
        'omochi.test.desktop.${DateTime.now().microsecondsSinceEpoch}';
    final store = NativeHubStore(namespace: namespace);
    await store.setOnline(false);
    final hub = HubSession(nativeStore: store);
    addTearDown(() async {
      await hub.stopNative();
      await store.clearTestNamespace();
    });
    final screenshotKey = GlobalKey();
    await tester.pumpWidget(
      RepaintBoundary(
        key: screenshotKey,
        child: MobileApp(links: mobileLinks(), session: hub),
      ),
    );
    final deadline = DateTime.now().add(const Duration(seconds: 30));
    while (hub.nativeIdentity == null &&
        hub.error == null &&
        DateTime.now().isBefore(deadline)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(hub.error, isNull);
    final identity = hub.nativeIdentity;
    expect(identity, matches(RegExp(r'^[a-f0-9]{64}$')));
    // Another runner must forward its URI before creating a second Dart/Hub
    // instance. This is a fictitious public test invitation; never enroll it.
    final invite =
        'omochi://invite/${base64Url.encode(utf8.encode(jsonEncode({
          'version': 4,
          'address': {'id': 'a' * 64, 'addrs': []},
          'secret': 'b' * 64,
          'expires': 4102444800,
        }))).replaceAll('=', '')}';
    final forwarded = await Process.run(Platform.resolvedExecutable, [
      invite,
    ]).timeout(const Duration(seconds: 15));
    expect(forwarded.exitCode, 0);
    final linkDeadline = DateTime.now().add(const Duration(seconds: 10));
    while (find.byType(AlertDialog).evaluate().isEmpty &&
        DateTime.now().isBefore(linkDeadline)) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byType(AlertDialog), findsOneWidget);
    expect(find.text('a' * 64), findsOneWidget);
    await tester.tap(
      find.descendant(
        of: find.byType(AlertDialog),
        matching: find.byType(TextButton),
      ),
    );
    await tester.pump(const Duration(milliseconds: 350));
    expect(hub.gateways, isEmpty);
    const screenshot = String.fromEnvironment('DESKTOP_SCREENSHOT');
    if (screenshot.isNotEmpty) {
      final boundary =
          screenshotKey.currentContext!.findRenderObject()
              as RenderRepaintBoundary;
      final image = await boundary.toImage();
      try {
        final bytes = await image.toByteData(format: ui.ImageByteFormat.png);
        await File(screenshot).writeAsBytes(bytes!.buffer.asUint8List());
      } finally {
        image.dispose();
      }
    }
    const hold = int.fromEnvironment('DESKTOP_HOLD_SECONDS');
    if (hold > 0) {
      await Future<void>.delayed(Duration(seconds: hold));
    }
    final document = await store.readIdentityDocument();
    expect(document, isNotNull);
    await hub.suspendNative();
    await hub.resumeNative();
    expect(hub.nativeIdentity, identity);
    await hub.stopNative();
    // Use another store and bridge, so persistence does not come from a Dart
    // object or an already-running native Hub.
    final reopened = SharedHubBridge(NativeHubStore(namespace: namespace));
    addTearDown(reopened.close);
    expect(
      (await reopened.request({'op': 'start', 'online': false}))['identity'],
      identity,
    );
    expect(await store.readIdentityDocument(), document);
    await reopened.close();
    await tester.pumpWidget(const SizedBox());
    debugPrint(
      'Desktop ${Platform.operatingSystem}: OS identity and bundled Hub verified',
    );
  }, skip: !Platform.isLinux && !Platform.isWindows);
}
