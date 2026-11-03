import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:json_schema_builder/json_schema_builder.dart';
import 'package:omochi_mobile/src/action_input.dart';

class _EchoFunction extends SynchronousClientFunction {
  const _EchoFunction();
  @override
  String get name => 'echo';
  @override
  String get description => 'Return the supplied value.';
  @override
  ClientFunctionReturnType get returnType => ClientFunctionReturnType.any;
  @override
  Schema get argumentSchema => S.object(properties: {'value': S.any()});
  @override
  Object? executeSync(JsonMap args, ExecutionContext context) => args['value'];
}

void main() {
  test(
    'nested arguments use relative paths and preserve resolved data',
    () async {
      final model = InMemoryDataModel();
      addTearDown(model.dispose);
      final payload = {'path': '/not-an-expression', 'call': 'not-a-function'};
      model.update(DataPath.root, {
        'rows': [
          {'enabled': false, 'payload': payload},
        ],
      });
      final context = DataContext(
        model,
        DataPath('/rows/0'),
        functions: [
          ...BasicFunctions.all,
          const _EchoFunction(),
          const ActionInputFunction(),
        ],
      );
      final expression = {
        'call': 'echo',
        'args': {
          'value': {
            'array': [
              {'path': 'enabled'},
              {
                'call': 'not',
                'args': {
                  'value': {'path': 'enabled'},
                },
              },
            ],
            'object': {
              'child': {'path': 'payload'},
            },
            'result': {
              'call': 'echo',
              'args': {
                'value': {'path': 'payload'},
              },
            },
            'empty': <String, Object?>{},
            'null': null,
          },
        },
      };
      final encoded = ActionInputFunction.encode(expression);
      final expected = {
        'array': [false, true],
        'object': {'child': payload},
        'result': payload,
        'empty': {},
        'null': null,
      };
      expect(await context.resolve(encoded).first, expected);
      context.update(DataPath('enabled'), true);
      expect(await context.resolve(encoded).first, {
        ...expected,
        'array': [true, false],
      });
      expect(expression['args'], containsPair('value', isA<Map>()));
      expect((expression['args'] as Map)['value']['array'][0], {
        'path': 'enabled',
      });
    },
  );

  test('action expression recursion is bounded', () async {
    final model = InMemoryDataModel();
    addTearDown(model.dispose);
    final context = DataContext(
      model,
      DataPath.root,
      functions: [const _EchoFunction(), const ActionInputFunction()],
    );
    Object? value = true;
    for (var i = 0; i < 102; i++) {
      value = [value];
    }
    await expectLater(
      context
          .resolve(
            ActionInputFunction.encode({
              'call': 'echo',
              'args': {'value': value},
            }),
          )
          .first,
      throwsStateError,
    );
  });
}
