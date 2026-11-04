import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'mobile_rendering_test.dart' show openService, session;
import 'native_layout_test.dart' show update;
import 'native_modal_test.dart' show modal;
import 'scoped_renderer_test.dart' show TestClient, TestScope;

class LifecycleClient extends TestClient {
  final transitions = <String>[];
  @override
  Future<void> suspend() async => transitions.add('suspend');
  @override
  Future<void> resume() async => transitions.add('resume');
}

void main() {
  testWidgets('focus-only resume preserves the open modal', (tester) async {
    final client = LifecycleClient();
    await openService(tester, session(client), client, TestScope('focus'));
    update(client, modal('focus'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open focus'));
    await tester.pumpAndSettle();
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    await tester.pumpAndSettle();
    expect(client.transitions, isEmpty);
    expect(find.text('Sheet focus'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });

  testWidgets('actual suspension resumes once, with duplicate events ignored', (
    tester,
  ) async {
    final client = LifecycleClient();
    await openService(tester, session(client), client, TestScope('pause'));
    for (final state in [
      AppLifecycleState.paused,
      AppLifecycleState.paused,
      AppLifecycleState.detached,
      AppLifecycleState.resumed,
      AppLifecycleState.resumed,
    ]) {
      tester.binding.handleAppLifecycleStateChanged(state);
    }
    await tester.pumpAndSettle();
    expect(client.transitions, ['suspend', 'resume']);
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });
}
