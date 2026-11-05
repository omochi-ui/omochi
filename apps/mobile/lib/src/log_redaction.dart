import 'dart:convert';

/// Wire diagnostics may include signed image URIs. Never retain their credentials,
/// query or fragment in the on-device log, even when embedded in a text field.
String diagnosticText(Object value) {
  final text = (value is String ? value : jsonEncode(value)).replaceAll(
    RegExp(r'omochi://(?:invite|share|owner)/[^\s"<>]+'),
    'omochi://invite/[redacted]',
  );
  return text.replaceAllMapped(
    RegExp(r'''https?://[^\s"<>]+''', caseSensitive: false),
    (match) {
      final raw = match[0]!;
      final uri = Uri.tryParse(raw);
      if (uri == null) return '[invalid HTTPS URI]';
      if (uri.hasQuery || uri.hasFragment || uri.userInfo.isNotEmpty) {
        return '${uri.replace(userInfo: '', query: '', fragment: '').toString().replaceFirst(RegExp(r'[?#]+$'), '')}[redacted]';
      }
      return raw;
    },
  );
}
