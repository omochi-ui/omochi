import 'package:flutter_test/flutter_test.dart';
import 'package:omochi_mobile/src/log_redaction.dart';

void main() {
  test('downstream custom URI credentials never enter diagnostics', () {
    for (final kind in ['share', 'owner']) {
      expect(
        diagnosticText('omochi://$kind/bearerSecret'),
        isNot(contains('bearerSecret')),
      );
    }
  });
  test('wire diagnostics redact signed URLs and credentials nested in image messages', () {
    final result = diagnosticText({
      'url': 'https://user:secret@images.example/a.png?signature=secret#token',
      'text': 'see https://images.example/b.png?key=secret and https://images.example/plain.png',
    });
    expect(result, isNot(contains('secret')));
    expect(result, isNot(contains('token')));
    expect(result, isNot(contains('user:')));
    expect(result, contains('https://images.example/a.png[redacted]'));
    expect(result, contains('https://images.example/plain.png'));
  });
}
