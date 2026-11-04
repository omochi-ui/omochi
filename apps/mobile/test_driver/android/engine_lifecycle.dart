// Used only by engine_lifecycle_test.py's disposable Flutter application.
import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

const marker = String.fromEnvironment('LEASE_ENGINE_MARKER');
const lease = MethodChannel('omochi/identity-lease');

void report(String message) => debugPrint('$marker $message');

class LifecycleProbe extends WidgetsBindingObserver {
  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    report('lifecycle ${state.name}');
  }
}

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  report('main');
  WidgetsBinding.instance.addObserver(LifecycleProbe());
  var token = await lease.invokeMethod<String>('acquire', 'omochi.test.engine');
  if (token == null) throw StateError('No lease');
  AppLinks().uriLinkStream.listen((uri) async {
    if (uri.path == '/finish') {
      report('finish');
      await SystemNavigator.pop();
    } else if (uri.path == '/check') {
      // The platform channel must still work after another Activity attaches.
      await lease.invokeMethod<void>('release', token);
      token = await lease.invokeMethod<String>('acquire', 'omochi.test.engine');
      if (token == null) throw StateError('No replacement lease');
      report('reacquired');
    }
  });
  runApp(
    const MaterialApp(home: Scaffold(body: Text('Isolated engine lease test'))),
  );
  report('owned');
}
