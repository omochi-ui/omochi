import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/messages.dart';

import 'scoped_renderer_test.dart' show TestClient, TestScope, TestStore;
import 'support/renderer_fixture.dart';

HubSession session(TestClient client) => HubSession(
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

Future<void> openService(
  WidgetTester tester,
  HubSession hub,
  TestClient client,
  TestScope scope,
) async {
  await tester.pumpWidget(
    MaterialApp(
      home: MobileHome(links: const Stream.empty(), session: hub),
    ),
  );
  await tester.pumpAndSettle();
  client.show(scope);
  await tester.pumpAndSettle();
  await tester.tap(find.text('counter'));
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('service screen expires callbacks when a Surface is replaced', (
    tester,
  ) async {
    final client = TestClient();
    final hub = session(client);
    final first = TestScope('screen:1');
    await openService(tester, hub, client, first);
    final oldClick = tester
        .widget<ButtonStyleButton>(
          find.descendant(
            of: find.byType(Surface),
            matching: find.byWidgetPredicate((w) => w is ButtonStyleButton),
          ),
        )
        .onPressed!;
    final second = TestScope('screen:2');
    client.show(second);
    await tester.pumpAndSettle();
    final expiredErrors = <Object>[];
    runZonedGuarded(oldClick, (error, stack) => expiredErrors.add(error));
    await tester.pumpAndSettle();
    // GenUI rejects dispatch from disposed Surface state before routing.
    for (final error in expiredErrors) {
      expect(error.toString(), contains('unmounted'));
    }
    expect(first.actions, isEmpty);
    expect(second.actions, isEmpty);
    await tester.tap(find.text('Run'));
    await tester.pumpAndSettle();
    expect(second.actions, hasLength(1));
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('service screen bounds weighted Columns across viewport sizes', (
    tester,
  ) async {
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('weighted:1'));
    client.onMessage({
      'version': 'v0.9',
      'updateComponents': {
        'surfaceId': counterId,
        'components': [
          {
            'id': 'root',
            'component': 'Column',
            'children': ['weighted'],
          },
          {...textComponent('weighted', 'Weighted content'), 'weight': 1},
        ],
      },
    });
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(tester.getSize(find.byType(Surface)).height, greaterThan(0));
    expect(tester.getSize(find.byType(Surface)).height.isFinite, isTrue);
    await tester.binding.setSurfaceSize(const Size(400, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    expect(find.text('Weighted content'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
  });

  for (final description in <Object?>[
    'Literal description',
    {'path': '/description'},
    {
      'call': 'formatString',
      'args': {'value': r'Photo: ${/description}'},
    },
    null,
    '',
  ]) {
    testWidgets('scoped image resolves description $description and variants', (
      tester,
    ) async {
      final semantics = tester.ensureSemantics();

      final client = TestClient();
      final hub = session(client);
      await hub.startNative();
      final fixture = File('tool/sdk_fixture.ts').readAsStringSync();
      final bytes = base64Decode(
        RegExp(r"Buffer.from\('([^']+)', 'base64'\)")
            .firstMatch(fixture)!
            .group(1)!,
      );
      final scope = TestScope('image:variants')
        ..imageFuture = Future.value(bytes);
      client.show(scope);
      void update(String? variant) => client.onMessage({
        'version': 'v0.9',
        'updateComponents': {
          'surfaceId': counterId,
          'components': [
            {
              'id': 'root',
              'component': 'Image',
              'url': 'omochi://resources/photo',
              'description': ?description,
              'variant': ?variant,
              'fit': 'cover',
            },
          ],
        },
      });
      client.onMessage(dataMessage(counterId, {'description': 'First'}));
      update('icon');
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: ListenableBuilder(
                listenable: hub,
                builder: (_, _) => Surface(
                  key: ValueKey(hub.contextForSurface(counterId).surfaceId),
                  surfaceContext: hub.contextForSurface(counterId),
                ),
              ),
            ),
          ),
        ),
      );
      Future<void> finishImage() async {
        await tester.pump();
        for (
          var i = 0;
          i < 50 && find.byType(RawImage).evaluate().isEmpty;
          i++
        ) {
          await tester.runAsync(
            () => Future<void>.delayed(const Duration(milliseconds: 10)),
          );
          await tester.pump();
        }
        await tester.pumpAndSettle();
      }

      await finishImage();
      String label(String value) =>
          description is String && description.isNotEmpty
          ? description
          : description is Map
          ? description.containsKey('call')
                ? 'Photo: $value'
                : value
          : messages.appImage;
      expect(find.bySemanticsLabel(label('First')), findsOneWidget);
      final requests = scope.images.length;
      client.onMessage(dataMessage(counterId, {'description': 'Second'}));
      await tester.pumpAndSettle();
      expect(find.bySemanticsLabel(label('Second')), findsOneWidget);
      expect(scope.images, hasLength(requests));
      for (final variant in <String?, double>{
        'icon': 32,
        'avatar': 32,
        'smallFeature': 50,
        'mediumFeature': 150,
        'largeFeature': 400,
        null: 150,
      }.entries) {
        update(variant.key);
        await finishImage();
        expect(
          tester.getSize(find.byType(RawImage)),
          Size.square(variant.value),
        );
        expect(
          tester.widget<RawImage>(find.byType(RawImage)).fit,
          BoxFit.cover,
        );
        if (variant.key == 'avatar') {
          expect(
            find.ancestor(
              of: find.byType(RawImage),
              matching: find.byType(ClipOval),
            ),
            findsOneWidget,
          );
        }
      }
      update('header');
      await finishImage();
      expect(
        tester.getSize(find.byType(RawImage)).width,
        tester.getSize(find.byType(Scaffold)).width,
      );
      await tester.pumpWidget(const SizedBox());
      await hub.stopNative();
      hub.dispose();
      semantics.dispose();
    });
  }
}
