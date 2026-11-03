import 'package:omochi_mobile/src/messages.dart';

import 'package:omochi_mobile/src/models.dart';

const basicCatalog =
    'https://a2ui.org/specification/v0_9/catalogs/basic/catalog.json';
const demoNode =
    '0000000000000000000000000000000000000000000000000000000000000001';
const counterId = '$demoNode~playground/counter';
const formId = '$demoNode~playground/form';
const noteId = '$demoNode~notes/main';

List<HubService> demoServices() => ['playground', 'notes']
    .map(
      (id) => HubService.fromJson({
        'id': '$demoNode~$id',
        'name': id == 'playground'
            ? messages.demoPlayground
            : messages.demoNotes,
        'status': 'connected',
        'node': {'id': demoNode, 'name': messages.demoDevice, 'local': true},
      }),
    )
    .toList();

List<Json> surfaceMessages(String id, List<Json> components, Json data) => [
  {
    'version': 'v0.9',
    'createSurface': {
      'surfaceId': id,
      'catalogId': basicCatalog,
      'sendDataModel': true,
    },
  },
  {
    'version': 'v0.9',
    'updateComponents': {'surfaceId': id, 'components': components},
  },
  dataMessage(id, data),
];
Json dataMessage(String id, Json data) => {
  'version': 'v0.9',
  'updateDataModel': {'surfaceId': id, 'path': '/', 'value': data},
};
Json textComponent(String id, dynamic text, {String variant = 'body'}) => {
  'id': id,
  'component': 'Text',
  'text': text,
  'variant': variant,
};
Json buttonComponent(
  String id,
  String label,
  String action, {
  Json context = const {},
}) => {
  'id': id,
  'component': 'Button',
  'child': label,
  'variant': 'primary',
  'action': {
    'event': {'name': action, 'context': context},
  },
};

List<Json> demoMessages() => [
  ...surfaceMessages(
    counterId,
    [
      textComponent('count', {'path': '/label'}, variant: 'h1'),
      textComponent('inc-label', messages.demoAddOne),
      buttonComponent('increment', 'inc-label', 'increment'),
      {
        'id': 'root',
        'component': 'Column',
        'children': ['count', 'increment'],
      },
    ],
    {'count': 0, 'label': messages.demoMoments(0)},
  ),
  ...surfaceMessages(
    formId,
    [
      {
        'id': 'task',
        'component': 'TextField',
        'label': messages.demoTask,
        'value': {'path': r'/$local/task'},
      },
      {
        'id': 'minutes',
        'component': 'Slider',
        'min': 5,
        'max': 60,
        'value': {'path': r'/$local/minutes'},
      },
      textComponent('start-label', messages.demoSetIntention),
      buttonComponent(
        'start',
        'start-label',
        'focus',
        context: {
          'input': {
            'task': {'path': r'/$local/task'},
            'minutes': {'path': r'/$local/minutes'},
          },
        },
      ),
      textComponent('result', {'path': '/result'}),
      {
        'id': 'root',
        'component': 'Column',
        'children': ['task', 'minutes', 'start', 'result'],
      },
    ],
    {
      r'$local': {'task': messages.demoSampleTask, 'minutes': 25},
      'result': '',
    },
  ),
  ...surfaceMessages(noteId, [
    textComponent('body', messages.demoNote),
    {
      'id': 'root',
      'component': 'Column',
      'children': ['body'],
    },
  ], {}),
];
