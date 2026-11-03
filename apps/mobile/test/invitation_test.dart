import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/protocol.dart';

void main() {
  test('shared current invitation fixtures agree with native validation', () {
    final fixture = jsonDecode(
      File('../../spec/fixtures/gateway-invitations.json').readAsStringSync(),
    ) as Map;
    for (final row in fixture['cases'] as List) {
      if (row['valid'] == true) {
        expect(
          () => GatewayInvitation.parse(row['input']),
          returnsNormally,
          reason: row['name'],
        );
      } else {
        expect(
          () => GatewayInvitation.parse(row['input']),
          throwsFormatException,
          reason: row['name'],
        );
      }
    }
  });
}
