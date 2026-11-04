import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';

import 'support/renderer_fixture.dart';

import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/models.dart';
import 'package:omochi_mobile/src/native/client_api.dart';
import 'package:omochi_mobile/src/native/image_budget.dart';
import 'package:omochi_mobile/src/native/protocol.dart';
import 'package:omochi_mobile/src/native/store.dart';

class TestStore extends NativeHubStore {}

class TestScope implements GatewaySurface {
  TestScope(this.renderId);
  @override
  final String renderId;
  @override
  String get surfaceId => counterId;
  final List<Json> actions = [];
  final List<String> images = [];
  Future<Uint8List>? imageFuture;
  bool imageCancelled = false;
  @override
  Future<void> send(Json message) async => actions.add(message);
  @override
  GatewayImageRequest image(String uri) {
    images.add(uri);
    return GatewayImageRequest(
      imageFuture ?? Future.error(StateError('fixture image')),
      () {
        imageCancelled = true;
      },
    );
  }
}

class TestClient extends GatewayClient {
  late void Function(Json) onMessage;
  late void Function(String) onRetract;
  TestScope? scope;
  Completer<void>? finishClose;
  @override
  bool get usesScopedSurfaces => true;
  @override
  GatewaySurface? surfaceFor(String id) => scope;
  @override
  String get identity => demoNode;
  @override
  bool get online => false;
  @override
  List<GatewayPeer> get peers => [];
  @override
  List<HubService> get services => demoServices();
  @override
  List<Json> get connections => [
    {'status': 'Connected'},
  ];
  @override
  Future<void> start() async => notifyListeners();
  @override
  Future<void> pair(String value) async {}
  @override
  Future<void> forget(String value) async {}
  @override
  Future<void> setOnline(bool value) async {}
  @override
  void retry(String id) {}
  @override
  Future<void> send(Json value) async => throw StateError('unscoped send');
  @override
  Future<Uint8List> resource(String route, String id) async =>
      throw StateError('unscoped image');
  @override
  Future<void> close() async {
    await finishClose?.future;
  }

  void show(TestScope next) {
    scope = next;
    for (final message in surfaceMessages(counterId, [
      {
        'id': 'root',
        'component': 'Column',
        'children': ['button'],
      },
      buttonComponent('button', 'label', 'run'),
      textComponent('label', 'Run'),
    ], {})) {
      onMessage(message);
    }
  }
}

void main() {
  testWidgets(
    'same logical Surface gets a new renderer ID and old callbacks cannot send',
    (tester) async {
      final client = TestClient();
      final hub = HubSession(
        nativeStore: TestStore(),
        gatewayClientFactory:
            ({
              required store,
              required onMessage,
              required onRetract,
              required onEvent,
            }) {
              client.onMessage = onMessage;
              client.onRetract = onRetract;
              return client;
            },
      );
      await hub.startNative();
      final first = TestScope('native:1');
      client.show(first);
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: ListenableBuilder(
              listenable: hub,
              builder: (_, _) => Surface(
                key: ValueKey(hub.contextForSurface(counterId).surfaceId),
                surfaceContext: hub.contextForSurface(counterId),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      final oldContext = hub.contextForSurface(counterId);
      final oldClick = tester
          .widget<ButtonStyleButton>(
            find.byWidgetPredicate((w) => w is ButtonStyleButton),
          )
          .onPressed!;
      oldClick();
      await tester.pumpAndSettle();
      expect(first.actions, hasLength(1));
      expect(first.actions.single['action']['surfaceId'], counterId);
      final second = TestScope('native:2');
      client.show(second);
      await tester.pumpAndSettle();
      expect(hub.surfaceIds, [counterId]);
      expect(hub.contextForSurface(counterId).surfaceId, 'native:2');
      oldContext.handleUiEvent(
        UserActionEvent.fromMap({
          'surfaceId': oldContext.surfaceId,
          'name': 'run',
          'sourceComponentId': 'button',
          'timestamp': DateTime.now().toUtc().toIso8601String(),
        }),
      );
      await tester.pumpAndSettle();
      expect(first.actions, hasLength(1));
      expect(second.actions, isEmpty);
      await tester.tap(find.text('Run'));
      await tester.pumpAndSettle();
      expect(second.actions, hasLength(1));
      await tester.pumpWidget(const SizedBox());
      await hub.stopNative();
      hub.dispose();
    },
  );
  for (final markdown in [false, true]) {
    testWidgets(
      'Image and Markdown ($markdown) use captured scopes and release pixels',
      (tester) async {
        final client = TestClient();
        final hub = HubSession(
          nativeStore: TestStore(),
          gatewayClientFactory:
              ({
                required store,
                required onMessage,
                required onRetract,
                required onEvent,
              }) {
                client.onMessage = onMessage;
                client.onRetract = onRetract;
                return client;
              },
        );
        await hub.startNative();
        final pending = Completer<Uint8List>();
        final first = TestScope('image:1')..imageFuture = pending.future;
        const uri = 'https://images.example/image.png?signature=private#token';
        void show(TestScope scope) {
          client.show(scope);
          client.onMessage({
            'version': 'v0.9',
            'updateComponents': {
              'surfaceId': counterId,
              'components': [
                if (markdown)
                  {
                    'id': 'root',
                    'component': 'Text',
                    'text': '![Example]($uri)',
                  }
                else
                  {'id': 'root', 'component': 'Image', 'url': uri},
              ],
            },
          });
        }

        show(first);
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: ListenableBuilder(
                listenable: hub,
                builder: (_, _) => hub.surfaceIds.isEmpty
                    ? const SizedBox()
                    : Surface(
                        key: ValueKey(
                          hub.contextForSurface(counterId).surfaceId,
                        ),
                        surfaceContext: hub.contextForSurface(counterId),
                      ),
              ),
            ),
          ),
        );
        await tester.pump();
        expect(first.images, [markdown ? uri.split('#').first : uri]);
        expect(hub.log.any((entry) => entry.body.contains('private')), false);
        client.onMessage({
          'version': 'v0.9',
          'deleteSurface': {'surfaceId': counterId},
        });
        await tester.pump();
        expect(first.imageCancelled, true);
        pending.complete(Uint8List(0));
        await tester.pump();
        expect(NativeImageBudget.shared.loading, 0);
        expect(NativeImageBudget.shared.decodedBytes, 0);
        final fixture = File('tool/sdk_fixture.ts').readAsStringSync();
        final imageBytes = base64Decode(
          RegExp(r"Buffer.from\('([^']+)', 'base64'\)")
              .firstMatch(fixture)!
              .group(1)!,
        );
        final before = PaintingBinding.instance.imageCache.currentSizeBytes;
        final failure = Completer<Uint8List>();
        final second = TestScope('image:2')..imageFuture = failure.future;
        show(second);
        await tester.pump();
        failure.completeError(StateError('temporary image failure'));
        await tester.pump();
        await tester.pump();
        expect(find.byType(TextButton), findsOneWidget);
        second.imageFuture = Future.value(imageBytes);
        await tester.tap(find.byType(TextButton));
        for (
          var i = 0;
          i < 50 && find.byType(RawImage).evaluate().isEmpty;
          i++
        ) {
          await tester.pump();
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 10)),
          );
        }
        await tester.pumpAndSettle();
        expect(tester.widget<RawImage>(find.byType(RawImage)).image!.width, 64);
        expect(NativeImageBudget.shared.decodedBytes, 64 * 32 * 4);
        expect(PaintingBinding.instance.imageCache.currentSizeBytes, before);
        await tester.pumpWidget(const SizedBox());
        expect(NativeImageBudget.shared.decodedBytes, 0);
        await hub.stopNative();
        hub.dispose();
      },
    );
  }
  test('a replacement client waits for the previous Hub to stop', () async {
    var created = 0;
    final first = TestClient()..finishClose = Completer<void>();
    final second = TestClient();
    final hub = HubSession(
      nativeStore: TestStore(),
      gatewayClientFactory:
          ({
            required store,
            required onMessage,
            required onRetract,
            required onEvent,
          }) {
            final client = created++ == 0 ? first : second;
            client.onMessage = onMessage;
            client.onRetract = onRetract;
            return client;
          },
    );
    await hub.startNative();
    final replacement = hub.startNative();
    await Future<void>.delayed(Duration.zero);
    expect(created, 1);
    first.finishClose!.complete();
    await replacement;
    expect(created, 2);
    await hub.stopNative();
    hub.dispose();
  });
}
