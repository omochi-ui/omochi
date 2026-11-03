import 'dart:async';
import 'dart:convert';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:integration_test/integration_test.dart';
import 'package:omochi_mobile/src/native/bridge.dart';
import 'package:omochi_mobile/src/native/store.dart';

class FaultStore extends NativeHubStore {
  FaultStore()
    : super(
        namespace:
            'omochi.test.storage.${DateTime.now().microsecondsSinceEpoch}',
      );
  String? fault;
  Completer<void>? entered, finish;
  @override
  Future<String> commitIdentityDocument(
    String? expected,
    String replacement,
  ) async {
    entered?.complete();
    entered = null;
    await finish?.future;
    final mode = fault;
    fault = null;
    if (mode == 'failed') return 'failed';
    final outcome = await super.commitIdentityDocument(expected, replacement);
    return mode == 'unknown' && outcome == 'committed' ? 'unknown' : outcome;
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const fixtureUrl = String.fromEnvironment(
    'FIXTURE_URL',
    defaultValue: 'http://127.0.0.1:5559',
  );
  testWidgets(
    'OS existing corruption is an error and close waits for the dispatched secure save',
    (tester) async {
      await tester.pumpWidget(const SizedBox());
      final store = FaultStore();
      addTearDown(store.clearTestNamespace);
      const corrupt = '{"peers":[]}';
      await store.secure.write(
        key: '${store.namespace}.identity',
        value: corrupt,
      );
      var bridge = SharedHubBridge(store);
      await expectLater(
        bridge.request({'op': 'start', 'online': false}),
        throwsStateError,
      );
      expect(await store.readIdentityDocument(), corrupt);
      await bridge.close();
      await store.clearTestNamespace();
      final entered = Completer<void>();
      store.entered = entered;
      store.finish = Completer<void>();
      bridge = SharedHubBridge(store);
      final started = bridge.request({'op': 'start', 'online': false});
      await entered.future;
      var closed = false;
      final closing = bridge.close().then((_) => closed = true);
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(closed, false);
      store.finish!.complete();
      final identity = (await started)['identity'];
      await closing;
      final document = await store.readIdentityDocument();
      final reopened = SharedHubBridge(store);
      expect(
        (await reopened.request({'op': 'start', 'online': false}))['identity'],
        identity,
      );
      expect(await store.readIdentityDocument(), document);
      await reopened.close();
    },
  );
  testWidgets(
    'OS enrollment save failure, unknown result, forget failure and re-pair stay fail-closed',
    (tester) async {
      await tester.pumpWidget(const SizedBox());
      final store = FaultStore();
      final bridge = SharedHubBridge(store);
      addTearDown(() async {
        await bridge.close();
        await store.clearTestNamespace();
      });
      final identity = (await bridge.request({
        'op': 'start',
        'online': true,
      }))['identity'];
      final initial = await store.readIdentityDocument();
      final response = await http
          .post(Uri.parse('$fixtureUrl/start'))
          .timeout(const Duration(seconds: 40));
      expect(response.statusCode, 200);
      final invitation =
          (jsonDecode(response.body)['invitations'] as List).first;
      store.fault = 'failed';
      await expectLater(
        bridge.request({'op': 'pair', 'invitation': invitation}),
        throwsStateError,
      );
      expect(await store.readIdentityDocument(), initial);
      var state = await bridge.request({'op': 'state'});
      expect(state['peers'], isEmpty);
      expect(state['services'], isEmpty);
      store.fault = 'unknown';
      await expectLater(
        bridge.request({'op': 'pair', 'invitation': invitation}),
        throwsStateError,
      );
      await expectLater(bridge.request({'op': 'state'}), throwsStateError);
      expect(
        (jsonDecode((await store.readIdentityDocument())!)['peers'] as List),
        hasLength(1),
      );
      await bridge.request({'op': 'reload'});
      for (var i = 0; i < 400; i++) {
        state = await bridge.request({'op': 'state'});
        if ((state['services'] as List).isNotEmpty) break;
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      expect(state['services'], hasLength(1));
      expect(state['identity'], identity);
      final peer = state['peers'][0]['address']['id'];
      await bridge.request({'op': 'subscribe'});
      final handle = (await bridge.request({'op': 'next'}))['handle'];
      expect(handle, isNotNull);
      store.fault = 'failed';
      await expectLater(
        bridge.request({'op': 'forget', 'id': peer}),
        throwsStateError,
      );
      state = await bridge.request({'op': 'state'});
      expect(state['services'], isEmpty);
      expect(state['peers'], hasLength(1));
      await expectLater(
        bridge.request({
          'op': 'action',
          'handle': handle,
          'message': {
            'version': 'v0.9',
            'action': {'name': 'run'},
          },
        }),
        throwsStateError,
      );
      await bridge.request({'op': 'forget', 'id': peer});
      expect((await bridge.request({'op': 'state'}))['peers'], isEmpty);
      await bridge.request({'op': 'pair', 'invitation': invitation});
      state = await bridge.request({'op': 'state'});
      expect(state['peers'], hasLength(1));
      expect(state['identity'], identity);
      await expectLater(
        bridge.request({
          'op': 'action',
          'handle': handle,
          'message': {
            'version': 'v0.9',
            'action': {'name': 'run'},
          },
        }),
        throwsStateError,
      );
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
