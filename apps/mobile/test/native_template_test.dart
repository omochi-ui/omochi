import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:omochi_mobile/src/models.dart';

import 'native_layout_test.dart' show LayoutFixture;
import 'support/renderer_fixture.dart';

void main() {
  for (final axis in ['Column', 'Row']) {
    for (final key in <String?>['a/b', 'a~b', 'a~1b', 'a~/b', null]) {
      testWidgets(
        '$axis template preserves bindings and edits for ${key ?? 'list index'}',
        (tester) async {
          final fixture = LayoutFixture();
          addTearDown(() => fixture.close(tester));
          final row = <String, Object?>{
            'label': 'Select row',
            'value': 'Original',
            'nullable': null,
          };
          Object rows(Json value) => key == null ? [value] : {key: value};
          await fixture.mount(
            tester,
            [
              {
                'id': 'root',
                'component': axis,
                'children': {'path': '/rows', 'componentId': 'item'},
              },
              {
                'id': 'item',
                'component': 'Column',
                'weight': 1,
                'children': ['input', 'select'],
              },
              {
                'id': 'input',
                'component': 'TextField',
                'label': 'Edit row',
                'value': {'path': 'value'},
              },
              buttonComponent(
                'select',
                'label',
                'select',
                context: {
                  'input': {
                    'value': {'path': 'value'},
                    'nullable': {'path': 'nullable'},
                    'row': {'path': ''},
                  },
                },
              ),
              textComponent('label', {'path': 'label'}),
            ],
            data: {'rows': rows(row)},
          );

          expect(find.text('Select row'), findsOneWidget);
          expect(find.text('Original'), findsOneWidget);
          expect(fixture.hub.surfaceErrors, isEmpty);
          await tester.enterText(find.byType(TextField), 'Edited');
          await tester.pumpAndSettle();
          final edited = {...row, 'value': 'Edited'};
          expect(
            fixture.hub
                .contextForSurface(counterId)
                .dataModel
                .getValue<Object?>(DataPath('/rows')),
            rows(edited),
          );
          await tester.tap(find.text('Select row'));
          await tester.pumpAndSettle();
          expect(fixture.scope.actions, hasLength(1));
          expect(fixture.scope.actions.single['action']['context']['input'], {
            'value': 'Edited',
            'nullable': null,
            'row': edited,
          });
          expect(fixture.hub.surfaceErrors, isEmpty);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }
}
