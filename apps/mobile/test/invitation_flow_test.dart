import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';

import 'scoped_renderer_test.dart' show TestClient, TestStore;

class PairClient extends TestClient {
  final paired = <String>[];
  @override
  Future<void> pair(String value) async {
    paired.add(value);
  }
}

void main() {
  testWidgets(
    'out of range invitation shows an error without opening confirmation',
    (tester) async {
      final fixture = jsonDecode(
        File('../../spec/fixtures/gateway-invitations.json').readAsStringSync(),
      );
      const prefix = 'omochi://invite/';
      final value = jsonDecode(
        utf8.decode(
          base64Url.decode(
            base64Url.normalize(
              (fixture['cases'][0]['input'] as String).substring(prefix.length),
            ),
          ),
        ),
      ) as Map<String, dynamic>;
      value['expires'] = 8640000000001;
      final input =
          '$prefix${base64Url.encode(utf8.encode(jsonEncode(value))).replaceAll('=', '')}';
      final links = StreamController<Uri>();
      addTearDown(links.close);
      final client = PairClient();
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
      links.add(Uri.parse(input));
      await tester.pumpWidget(
        MaterialApp(
          home: MobileHome(links: links.stream, session: hub),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.byType(AlertDialog), findsNothing);
      expect(
        find.textContaining('Invalid or expired Gateway invitation'),
        findsOneWidget,
      );
      expect(client.paired, isEmpty);
      await tester.pumpWidget(const SizedBox());
    },
  );

  testWidgets(
    'cold and warm invitations require confirmation and deduplicate',
    (tester) async {
      final fixture = jsonDecode(
        File('../../spec/fixtures/gateway-invitations.json').readAsStringSync(),
      );
      final link = Uri.parse(fixture['cases'][0]['input']);
      final links = StreamController<Uri>();
      addTearDown(links.close);
      final client = PairClient();
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
      links.add(link);
      await tester.pumpWidget(
        MaterialApp(
          home: MobileHome(links: links.stream, session: hub),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Add to this device'), findsOneWidget);
      expect(client.paired, isEmpty);
      links.add(link);
      await tester.pumpAndSettle();
      expect(find.byType(AlertDialog), findsOneWidget);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(client.paired, isEmpty);
      links.add(link);
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add to this device'));
      await tester.pumpAndSettle();
      expect(client.paired, [link.toString()]);
      await tester.pumpWidget(const SizedBox());
    },
  );
}
