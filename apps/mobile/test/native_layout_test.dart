import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:omochi_mobile/src/hub_session.dart';
import 'package:omochi_mobile/src/models.dart';

import 'mobile_rendering_test.dart' show openService, session;
import 'scoped_renderer_test.dart' show TestClient, TestScope;
import 'support/renderer_fixture.dart';

void update(TestClient client, List<Json> components) => client.onMessage({
  'version': 'v0.9',
  'updateComponents': {'surfaceId': counterId, 'components': components},
});

List<Json> longColumn({String id = 'root'}) => [
  {
    'id': id,
    'component': 'Column',
    'children': [for (var i = 0; i < 40; i++) 'line$i', 'finish'],
  },
  for (var i = 0; i < 40; i++) textComponent('line$i', 'Line $i'),
  buttonComponent('finish', 'finish-label', 'finish'),
  textComponent('finish-label', 'Finish'),
];

Future<void> finish(WidgetTester tester) async {
  await tester.ensureVisible(find.text('Finish'));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Finish'));
  await tester.pumpAndSettle();
}

Finder component(String id) => find.byKey(ValueKey(id));

class LayoutFixture {
  final client = TestClient();
  late final HubSession hub = session(client);
  final scope = TestScope('layout:1');

  Future<void> mount(
    WidgetTester tester,
    List<Json> components, {
    Json data = const {},
    double height = 300,
    TextDirection direction = TextDirection.ltr,
  }) async {
    await hub.startNative();
    client.show(scope);
    update(client, components);
    client.onMessage(dataMessage(counterId, data));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Directionality(
            textDirection: direction,
            child: Align(
              alignment: Alignment.topLeft,
              child: SizedBox(
                width: 320,
                height: height,
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
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  Future<void> close(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox());
    hub.dispose();
  }
}

List<Json> modalComponents() => [
  {'id': 'root', 'component': 'Modal', 'trigger': 'open', 'content': 'sheet'},
  textComponent('open', 'Open sheet'),
  ...longColumn(id: 'sheet'),
];

void main() {
  for (final wrapper in [
    'direct',
    'card',
    'nestedCards',
    'buttonCard',
    'rowCard',
  ]) {
    for (final template in [false, true]) {
      testWidgets(
        'unweighted stretch Row has finite height ($wrapper, template=$template)',
        (tester) async {
          final fixture = LayoutFixture();
          await fixture.mount(
            tester,
            [
              {
                'id': 'root',
                'component': 'Column',
                'children': template
                    ? {
                        'path': '/rows',
                        'componentId': wrapper == 'direct' ? 'row' : 'wrapper',
                      }
                    : [wrapper == 'direct' ? 'row' : 'wrapper', 'finish'],
              },
              if (wrapper == 'card')
                {'id': 'wrapper', 'component': 'Card', 'child': 'row'},
              if (wrapper == 'nestedCards') ...[
                {'id': 'wrapper', 'component': 'Card', 'child': 'inner'},
                {'id': 'inner', 'component': 'Card', 'child': 'row'},
              ],
              if (wrapper == 'buttonCard') ...[
                buttonComponent('wrapper', 'inner', 'wrapped'),
                {'id': 'inner', 'component': 'Card', 'child': 'row'},
              ],
              if (wrapper == 'rowCard') ...[
                {
                  'id': 'wrapper',
                  'component': 'Row',
                  'children': ['inner'],
                },
                {'id': 'inner', 'component': 'Card', 'child': 'row'},
              ],
              {
                'id': 'row',
                'component': 'Row',
                'align': 'stretch',
                'children': ['left', 'right'],
              },
              textComponent('left', 'Left'),
              textComponent('right', 'Right'),
              buttonComponent('finish', 'label', 'finish'),
              textComponent('label', 'Finish'),
            ],
            data: {
              'rows': [1],
            },
          );
          expect(tester.takeException(), isNull);
          final rowHeight = tester.getSize(component('row')).height;
          expect(rowHeight, greaterThan(0));
          expect(rowHeight, lessThanOrEqualTo(300));
          if (wrapper == 'direct') expect(rowHeight, 300);
          expect(tester.getSize(component('left')).height, rowHeight);
          expect(tester.getSize(component('right')).height, rowHeight);
          if (!template) {
            await finish(tester);
            expect(fixture.scope.actions.single['action']['name'], 'finish');
          }
          // Changing alignment must also update the parent's sizing requirement.
          update(fixture.client, [
            {
              'id': 'row',
              'component': 'Row',
              'align': 'start',
              'children': ['left', 'right'],
            },
          ]);
          await tester.pumpAndSettle();
          expect(tester.getSize(component('row')).height, lessThan(rowHeight));
          update(fixture.client, [
            {
              'id': 'row',
              'component': 'Row',
              'align': 'stretch',
              'children': ['left', 'right'],
            },
          ]);
          await tester.pumpAndSettle();
          expect(tester.takeException(), isNull);
          expect(tester.getSize(component('row')).height, rowHeight);
          await fixture.close(tester);
        },
      );
    }
  }

  testWidgets('40-row Column scrolls to its final action', (tester) async {
    final client = TestClient();
    final hub = session(client);
    final scope = TestScope('scroll:1');
    await openService(tester, hub, client, scope);
    update(client, longColumn());
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await finish(tester);
    expect(scope.actions.single['action']['name'], 'finish');
    await tester.pumpWidget(const SizedBox());
  });

  testWidgets('weights divide space after natural children and spacing', (
    tester,
  ) async {
    final fixture = LayoutFixture();
    await fixture.mount(tester, [
      {
        'id': 'root',
        'component': 'Column',
        'children': ['header', 'one', 'two'],
      },
      textComponent('header', 'Header'),
      {...textComponent('one', 'One'), 'weight': 1},
      {...textComponent('two', 'Two'), 'weight': 2},
    ]);
    final remaining = 300 - tester.getSize(component('header')).height - 16;
    expect(
      tester.getSize(component('one')).height,
      closeTo(remaining / 3, .01),
    );
    expect(
      tester.getSize(component('two')).height,
      closeTo(remaining * 2 / 3, .01),
    );
    expect(tester.takeException(), isNull);
    await fixture.close(tester);
  });

  testWidgets('exhausted natural space gives weighted List a finite viewport', (
    tester,
  ) async {
    final fixture = LayoutFixture();
    await fixture.mount(tester, [
      {
        'id': 'root',
        'component': 'Column',
        'children': [for (var i = 0; i < 40; i++) 'line$i', 'items'],
      },
      for (var i = 0; i < 40; i++) textComponent('line$i', 'Line $i'),
      {
        'id': 'items',
        'component': 'List',
        'weight': 1,
        'children': [for (var i = 0; i < 40; i++) 'item$i', 'finish'],
      },
      for (var i = 0; i < 40; i++) textComponent('item$i', 'Item $i'),
      buttonComponent('finish', 'label', 'finish'),
      textComponent('label', 'Finish'),
    ]);
    expect(tester.takeException(), isNull);
    expect(tester.getSize(component('items')).height, 300);
    await finish(tester);
    expect(fixture.scope.actions.single['action']['name'], 'finish');
    await fixture.close(tester);
  });

  for (final weight in [null, 1]) {
    testWidgets('List weight $weight stays finite beside weighted content', (
      tester,
    ) async {
      final fixture = LayoutFixture();
      await fixture.mount(tester, [
        {
          'id': 'root',
          'component': 'Column',
          'children': ['one', 'items'],
        },
        {...textComponent('one', 'One'), 'weight': 1},
        {
          'id': 'items',
          'component': 'List',
          'weight': ?weight,
          'children': ['body'],
        },
        textComponent('body', 'Short list'),
      ]);
      expect(tester.takeException(), isNull);
      expect(tester.getSize(component('one')).height, 146);
      expect(
        tester.getSize(component('items')).height,
        weight == null ? lessThan(146) : equals(146),
      );
      await fixture.close(tester);
    });
  }

  for (final template in [false, true]) {
    testWidgets('nested Column scrolls with template=$template', (
      tester,
    ) async {
      final fixture = LayoutFixture();
      await fixture.mount(
        tester,
        [
          {
            'id': 'root',
            'component': 'Column',
            'children': ['heading', 'nested'],
          },
          textComponent('heading', 'Nested'),
          if (template) ...[
            {
              'id': 'nested',
              'component': 'Column',
              'children': {'path': '/rows', 'componentId': 'row'},
            },
            buttonComponent(
              'row',
              'label',
              'row',
              context: {
                'index': {'path': 'index'},
              },
            ),
            textComponent('label', {'path': 'label'}),
          ] else
            ...longColumn(id: 'nested'),
        ],
        data: {
          'rows': [
            for (var i = 0; i < 40; i++) {'label': 'Row $i', 'index': i},
          ],
        },
      );
      expect(tester.takeException(), isNull);
      if (template) {
        await tester.ensureVisible(find.text('Row 39'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Row 39'));
        await tester.pumpAndSettle();
        expect(fixture.scope.actions.single['action']['context']['index'], 39);
      } else {
        await finish(tester);
        expect(fixture.scope.actions.single['action']['name'], 'finish');
      }
      expect(tester.takeException(), isNull);
      await fixture.close(tester);
    });
  }

  testWidgets('weighted template follows map bindings and model updates', (
    tester,
  ) async {
    final fixture = LayoutFixture();
    await fixture.mount(
      tester,
      [
        {
          'id': 'root',
          'component': 'Column',
          'children': {'path': '/rows', 'componentId': 'row'},
        },
        {
          ...textComponent('row', {'path': 'label'}),
          'weight': 1,
        },
      ],
      data: {
        'rows': {
          'a': {'label': 'A'},
          'b': {'label': 'B'},
        },
      },
    );
    for (final element in component('row').evaluate()) {
      expect(
        tester.getSize(find.byElementPredicate((e) => e == element)).height,
        150,
      );
    }
    fixture.client.onMessage(
      dataMessage(counterId, {
        'rows': {
          'b': {'label': 'B updated'},
          'c': {'label': 'C'},
          'd': {'label': 'D'},
        },
      }),
    );
    await tester.pumpAndSettle();
    expect(find.text('A'), findsNothing);
    expect(find.text('B updated'), findsOneWidget);
    expect(component('row'), findsNWidgets(3));
    for (final element in component('row').evaluate()) {
      expect(
        tester.getSize(find.byElementPredicate((e) => e == element)).height,
        100,
      );
    }
    expect(tester.takeException(), isNull);
    await fixture.close(tester);
  });

  testWidgets('weighted nested Column bounds its implicitly flexible List', (
    tester,
  ) async {
    final fixture = LayoutFixture();
    await fixture.mount(tester, [
      {
        'id': 'root',
        'component': 'Column',
        'align': 'stretch',
        'children': ['header', 'nested'],
      },
      textComponent('header', 'Header'),
      {
        'id': 'nested',
        'component': 'Column',
        'weight': 1,
        'children': ['listHeader', 'items'],
      },
      textComponent('listHeader', 'List header'),
      {
        'id': 'items',
        'component': 'List',
        'children': [for (var i = 0; i < 40; i++) 'item$i', 'finish'],
      },
      for (var i = 0; i < 40; i++) textComponent('item$i', 'Item $i'),
      buttonComponent('finish', 'label', 'finish'),
      textComponent('label', 'Finish'),
    ]);
    final nestedHeight = 300 - tester.getSize(component('header')).height - 8;
    expect(tester.getSize(component('nested')), Size(320, nestedHeight));
    expect(tester.getSize(component('items')).height, lessThan(nestedHeight));
    await finish(tester);
    expect(fixture.scope.actions.single['action']['name'], 'finish');
    expect(tester.takeException(), isNull);
    await fixture.close(tester);
  });

  testWidgets('small viewport, text scaling and keyboard recalculate flex', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(320, 600));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    tester.platformDispatcher.textScaleFactorTestValue = 2;
    addTearDown(tester.platformDispatcher.clearTextScaleFactorTestValue);
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('keyboard:1'));
    update(client, [
      {
        'id': 'root',
        'component': 'Column',
        'children': ['input', 'weighted'],
      },
      {'id': 'input', 'component': 'TextField', 'label': 'Input', 'value': ''},
      {...textComponent('weighted', 'Weighted'), 'weight': 1},
    ]);
    await tester.pumpAndSettle();
    final before = tester.getSize(component('weighted')).height;
    await tester.tap(find.byType(TextField));
    tester.view.viewInsets = FakeViewPadding(
      bottom: 180 * tester.view.devicePixelRatio,
    );
    addTearDown(tester.view.resetViewInsets);
    await tester.pumpAndSettle();
    expect(tester.getSize(component('weighted')).height, lessThan(before));
    expect(tester.takeException(), isNull);
    update(client, longColumn());
    await tester.pumpAndSettle();
    await finish(tester);
    expect(tester.takeException(), isNull);
    await tester.pumpWidget(const SizedBox());
  });

  for (final direction in TextDirection.values) {
    testWidgets('alignment and spacing follow $direction', (tester) async {
      final fixture = LayoutFixture();
      await fixture.mount(tester, [
        {
          'id': 'root',
          'component': 'Column',
          'justify': 'spaceBetween',
          'align': 'end',
          'children': ['a', 'b'],
        },
        textComponent('a', 'A'),
        textComponent('b', 'B'),
      ], direction: direction);
      final bounds = tester.getRect(component('root'));
      final first = tester.getRect(component('a'));
      final last = tester.getRect(component('b'));
      expect(first.top, bounds.top);
      expect(last.bottom, bounds.bottom);
      expect(
        direction == TextDirection.ltr ? first.right : first.left,
        direction == TextDirection.ltr ? bounds.right : bounds.left,
      );
      expect(tester.takeException(), isNull);
      await fixture.close(tester);
    });
  }

  testWidgets('Modal Column scrolls to its final action', (tester) async {
    final client = TestClient();
    final hub = session(client);
    final scope = TestScope('modal:scroll');
    await openService(tester, hub, client, scope);
    update(client, modalComponents());
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open sheet'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
    await finish(tester);
    expect(scope.actions.single['action']['name'], 'finish');
    await tester.pumpWidget(const SizedBox());
    await tester.pumpAndSettle();
  });

  testWidgets('open Modal is removed with its Surface', (tester) async {
    final client = TestClient();
    final hub = session(client);
    await openService(tester, hub, client, TestScope('modal:1'));
    update(client, [
      {
        'id': 'root',
        'component': 'Modal',
        'trigger': 'open',
        'content': 'body',
      },
      textComponent('open', 'Open sheet'),
      textComponent('body', 'Owned sheet'),
    ]);
    await tester.pumpAndSettle();
    await tester.tap(find.text('Open sheet'));
    await tester.pumpAndSettle();
    expect(find.text('Owned sheet'), findsOneWidget);
    client.onMessage({
      'version': 'v0.9',
      'deleteSurface': {'surfaceId': counterId},
    });
    await tester.pumpAndSettle();
    expect(find.text('Owned sheet'), findsNothing);
    await tester.pumpWidget(const SizedBox());
  });
}
