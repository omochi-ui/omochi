import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/app.dart';
import 'package:omochi_mobile/src/hub_session.dart';

import 'invitation_flow_test.dart' show PairClient;
import 'scoped_renderer_test.dart' show TestStore;

HubSession session() => HubSession(
  nativeStore: TestStore(),
  gatewayClientFactory:
      ({
        required store,
        required onMessage,
        required onRetract,
        required onEvent,
      }) {
        return PairClient()
          ..onMessage = onMessage
          ..onRetract = onRetract;
      },
);

void main() {
  for (final scenario in [
    (locales: [const Locale('ja', 'JP')], language: 'ja'),
    (locales: [const Locale('en', 'US')], language: 'en'),
    (locales: [const Locale('fr', 'FR')], language: 'en'),
    (locales: [const Locale('fr'), const Locale('ja')], language: 'ja'),
  ]) {
    testWidgets('follows system languages ${scenario.locales}', (tester) async {
      tester.binding.platformDispatcher.localesTestValue = scenario.locales;
      addTearDown(tester.binding.platformDispatcher.clearLocalesTestValue);
      final links = StreamController<Uri>();
      addTearDown(links.close);
      links.add(Uri.parse('omochi://invite/invalid'));
      await tester.pumpWidget(
        MobileApp(links: links.stream, session: session()),
      );
      await tester.pumpAndSettle();

      final japanese = scenario.language == 'ja';
      expect(find.text(japanese ? 'アプリ' : 'Apps'), findsWidgets);
      expect(find.text(japanese ? '接続' : 'Connections'), findsOneWidget);
      final context = tester.element(find.byType(MobileHome));
      expect(Localizations.localeOf(context).languageCode, scenario.language);
      expect(
        MaterialLocalizations.of(context).cancelButtonLabel,
        japanese ? 'キャンセル' : 'Cancel',
      );

      expect(
        find.textContaining(
          japanese
              ? 'ゲートウェイへの招待が無効か、有効期限が切れています。'
              : 'Invalid or expired Gateway invitation',
        ),
        findsOneWidget,
      );
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
    });
  }

  testWidgets(
    'updates the screen and open dialog when system language changes',
    (tester) async {
      tester.binding.platformDispatcher.localesTestValue = [const Locale('en')];
      addTearDown(tester.binding.platformDispatcher.clearLocalesTestValue);
      await tester.pumpWidget(
        MobileApp(links: const Stream.empty(), session: session()),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Connections'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Add gateway'));
      await tester.pumpAndSettle();
      expect(find.text('Gateway invitation'), findsOneWidget);

      tester.binding.platformDispatcher.localesTestValue = [const Locale('ja')];
      await tester.pumpAndSettle();
      expect(find.text('ゲートウェイへの招待'), findsOneWidget);
      expect(find.text('キャンセル'), findsOneWidget);
      expect(find.text('続ける'), findsOneWidget);
      expect(find.text('Gateway invitation'), findsNothing);
      await tester.tap(find.text('キャンセル'));
      await tester.pumpAndSettle();
      expect(find.text('ゲートウェイを追加'), findsOneWidget);
      expect(find.text('インターネットリレー'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox());
      await tester.pump(const Duration(milliseconds: 300));
    },
  );
}
