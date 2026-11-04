import 'dart:convert';

import 'action_input.dart';
import 'models.dart';

/// GenUI 0.10 validates/resolves only top-level event context values.
/// Flatten bindings before rendering so template-relative paths still resolve in
/// GenUI's own DataContext, then restore Omochi's nested action input on submit.
/// References and calls use [ActionInputFunction] to validate missing paths and
/// resolve nested arguments at click time.
class ContextCodec {
  final Map<String, Map<String, Object?>> _shapes = {};
  void clear() => _shapes.clear();
  Json incoming(Json message) {
    final copy = jsonDecode(jsonEncode(message)) as Json;
    if (copy['deleteSurface'] case final Map deleted) {
      _shapes.remove(deleted['surfaceId']);
    }
    if (copy['createSurface'] case final Map created) {
      _shapes.remove(created['surfaceId']);
    }
    if (copy['updateComponents'] case final Map update) {
      final shapes = _shapes.putIfAbsent(
        update['surfaceId'] as String,
        () => {},
      );
      for (final component in update['components'] as List) {
        final id = component['id'] as String;
        shapes.remove(id);
        final event = component['action']?['event'];
        if (event is! Map || event['context'] is! Map) continue;
        final values = <String, dynamic>{};
        Object? flatten(Object? value) {
          if (value is Map &&
              !value.containsKey('path') &&
              !value.containsKey('call')) {
            return value.map(
              (key, child) => MapEntry(key as String, flatten(child)),
            );
          }
          if (value is List) return value.map(flatten).toList();
          final key = 'v${values.length}';
          values[key] =
              value is Map &&
                  (value.containsKey('path') || value.containsKey('call'))
              ? ActionInputFunction.encode(value)
              : value;
          return _Slot(key);
        }

        shapes[id] = flatten(event['context']);
        event['context'] = values;
      }
    }
    return copy;
  }

  Json outgoing(Json action) {
    final shape = _shapes[action['surfaceId']]?[action['sourceComponentId']];
    if (shape == null) return action;
    final values = action['context'] as Map? ?? {};
    Object? restore(Object? value) {
      if (value is _Slot) return values[value.key];
      if (value is Map) {
        return value.map((key, child) => MapEntry(key, restore(child)));
      }
      if (value is List) return value.map(restore).toList();
      return value;
    }

    return {...action, 'context': restore(shape)};
  }
}

class _Slot {
  const _Slot(this.key);
  final String key;
}
