import 'dart:convert';

import 'package:a2ui_core/a2ui_core.dart' as core;
import 'package:genui/genui.dart';
import 'package:json_schema_builder/json_schema_builder.dart';

/// Evaluates action calls in the clicking component's (possibly nested) context.
/// The encoded expression keeps GenUI from partially evaluating its arguments.
class ActionInputFunction implements ClientFunction {
  const ActionInputFunction();

  static const functionName = 'omochiResolveActionInput';

  static JsonMap encode(Map expression) => {
    'call': functionName,
    'args': {'expression': jsonEncode(expression)},
    if (expression.containsKey('returnType'))
      'returnType': expression['returnType'],
  };

  @override
  String get name => functionName;
  @override
  String get description =>
      'Resolve an encoded Omochi action input expression.';
  @override
  ClientFunctionReturnType get returnType => ClientFunctionReturnType.any;
  @override
  Schema get argumentSchema => S.object(
    properties: {'expression': S.string()},
    required: ['expression'],
  );

  @override
  Stream<Object?> execute(JsonMap args, ExecutionContext context) =>
      Stream.fromFuture(
        _resolve(jsonDecode(args['expression'] as String), context, 0),
      );

  Future<Object?> _resolve(
    Object? value,
    ExecutionContext context,
    int depth,
  ) async {
    if (depth > 100) {
      throw StateError('Action input exceeds maximum expression depth');
    }
    if (value is List) {
      return [
        for (final child in value) await _resolve(child, context, depth + 1),
      ];
    }
    if (value is! Map) return value;
    if (value.containsKey('path')) {
      // Resolved model values are data, even when they contain path/call keys.
      return _resolveReference(value['path'] as String, context);
    }
    if (value.containsKey('call')) {
      final name = value['call'] as String;
      final function = context.getFunction(name);
      if (function == null || name == functionName) {
        throw StateError('Unsupported action input function: $name');
      }
      final arguments = value['args'] as Map? ?? const <String, Object?>{};
      final resolved = <String, Object?>{
        for (final entry in arguments.entries)
          entry.key as String: await _resolve(entry.value, context, depth + 1),
      };
      // Invoke directly so resolved arguments and results are not evaluated again.
      return function.execute(resolved, context).first;
    }
    return <String, Object?>{
      for (final entry in value.entries)
        entry.key as String: await _resolve(entry.value, context, depth + 1),
    };
  }

  Object? _resolveReference(String reference, ExecutionContext context) {
    final path = context.resolvePath(DataPath(reference));
    // GenUI returns null for both absent paths and existing JSON null. Walk the
    // root snapshot using the underlying model's JSON Pointer parsing so only
    // absent references fail, including in template-relative contexts.
    Object? current = context.getValue<Object?>(DataPath.root);
    for (final segment in core.DataPath.parse(path.toString()).segments) {
      if (current is Map && current.containsKey(segment)) {
        current = current[segment];
      } else if (current is List) {
        final index = int.tryParse(segment);
        if (index == null || index < 0 || index >= current.length) {
          throw StateError('Missing action input reference: $reference');
        }
        current = current[index];
      } else {
        throw StateError('Missing action input reference: $reference');
      }
    }
    return current;
  }
}
