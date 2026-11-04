import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/native/protocol.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('invitation expiry must fit the displayed Dart date range', () {
    final fixture = jsonDecode(
      File('../../spec/fixtures/gateway-invitations.json').readAsStringSync(),
    );
    final prefix = 'omochi://invite/';
    final link = fixture['cases'][0]['input'] as String;
    final value = jsonDecode(
      utf8.decode(
        base64Url.decode(base64Url.normalize(link.substring(prefix.length))),
      ),
    ) as Map<String, dynamic>;
    for (final expires in [8640000000000, 8640000000001, 9223372036854775807]) {
      value['expires'] = expires;
      final input =
          '$prefix${base64Url.encode(utf8.encode(jsonEncode(value))).replaceAll('=', '')}';
      if (expires == 8640000000000) {
        final parsed = GatewayInvitation.parse(input);
        expect(
          () =>
              DateTime.fromMillisecondsSinceEpoch(parsed.expires * 1000)
                  .toLocal()
                  .toString(),
          returnsNormally,
        );
      } else {
        expect(() => GatewayInvitation.parse(input), throwsFormatException);
      }
    }
  });

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
