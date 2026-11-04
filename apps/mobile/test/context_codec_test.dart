import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/action_input.dart';
import 'package:omochi_mobile/src/context_codec.dart';

void main() {
  test('nested arrays, bindings and empty structures round-trip without mutation', () {
    final codec = ContextCodec();
    final original = {
      'version': 'v0.9',
      'updateComponents': {
        'surfaceId': 's',
        'components': [
          {
            'id': 'b',
            'component': 'Button',
            'action': {
              'event': {
                'name': 'run',
                'context': {
                  'input': {
                    'rows': [
                      {'path': 'relative/name'},
                      7,
                    ],
                    'empty': <String, dynamic>{},
                  },
                },
              },
            },
          },
        ],
      },
    };
    final incoming = codec.incoming(original);
    final context =
        incoming['updateComponents']['components'][0]['action']['event']['context'];
    expect(context, {
      'v0': ActionInputFunction.encode({'path': 'relative/name'}),
      'v1': 7,
    });
    final restored = codec.outgoing({
      'surfaceId': 's',
      'sourceComponentId': 'b',
      'context': {'v0': 'resolved', 'v1': 7},
    });
    expect(restored['context'], {
      'input': {
        'rows': ['resolved', 7],
        'empty': {},
      },
    });
    expect(original['updateComponents'].toString(), contains('relative/name'));
    codec.incoming({
      'version': 'v0.9',
      'deleteSurface': {'surfaceId': 's'},
    });
    expect(
      codec.outgoing({
        'surfaceId': 's',
        'sourceComponentId': 'b',
        'context': {'native': 1},
      })['context'],
      {'native': 1},
    );
  });
}
