import 'package:flutter_test/flutter_test.dart';
import 'package:genui/genui.dart';
import 'package:json_schema_builder/json_schema_builder.dart';
import 'package:omochi_mobile/src/action_input.dart';
import 'package:omochi_mobile/src/context_codec.dart';

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
  test('codec rejects missing references at every expression boundary', () async {
    final model = InMemoryDataModel();
    addTearDown(model.dispose);
    model.update(DataPath.root, {
      'rows': [
        {'nullable': null, 'scalar': true},
      ],
    });
    final context = DataContext(
      model,
      DataPath('/rows/0'),
      functions: [const _EchoFunction(), const ActionInputFunction()],
    );
    final codec = ContextCodec();
    Future<Object?> resolve(Object? expression) async {
      final incoming = codec.incoming({
        'updateComponents': {
          'surfaceId': 's',
          'components': [
            {
              'id': 'button',
              'action': {
                'event': {
                  'context': {'input': expression},
                },
              },
            },
          ],
        },
      });
      final definition =
          incoming['updateComponents']['components'][0]['action']['event']['context']
              as JsonMap;
      final resolved = await resolveContext(context, definition);
      return codec.outgoing({
        'surfaceId': 's',
        'sourceComponentId': 'button',
        'context': resolved,
      })['context']['input'];
    }

    for (final reference in [
      '/missing',
      'missing',
      'nullable/child',
      'scalar/child',
      '/rows/1',
      '/rows/-1',
      '/rows/not-an-index',
    ]) {
      final binding = {'path': reference};
      for (final expression in [
        binding,
        {
          'nested': [binding],
        },
        {
          'call': 'echo',
          'args': {
            'value': {
              'nested': [binding],
            },
          },
        },
      ]) {
        await expectLater(
          resolve(expression),
          throwsA(
            isA<StateError>().having(
              (error) => error.message,
              'message',
              'Missing action input reference: $reference',
            ),
          ),
        );
      }
    }
    // Existing null is data, both through a reference and a function result.
    expect(await resolve({'path': 'nullable'}), isNull);
    expect(
      await resolve({
        'call': 'echo',
        'args': {
          'value': {'path': 'nullable'},
        },
      }),
      isNull,
    );
    expect(await resolve({'path': 'scalar'}), true);
  });

  test(
    'references follow model JSON Pointer semantics and keep null entries',
    () async {
      final model = InMemoryDataModel();
      addTearDown(model.dispose);
      final payload = {'path': '/missing', 'call': 'unknown'};
      model.update(DataPath.root, {
        'rows': {
          'item/key': {
            '~value': [null, payload],
          },
        },
      });
      final context = DataContext(
        model,
        DataPath('/rows/item~1key'),
        functions: [const ActionInputFunction()],
      );
      Future<Object?> resolve(String path) =>
          context.resolve(ActionInputFunction.encode({'path': path})).first;
      expect(await resolve('~0value/0'), isNull);
      expect(await resolve('~0value/1'), payload);
      expect(await resolve(''), {
        '~value': [null, payload],
      });
      expect(await resolve('/'), model.getValue<Object?>(DataPath.root));
    },
  );

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
