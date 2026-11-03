import '../l10n/generated/app_messages.dart';
import '../l10n/generated/app_messages_en.dart';

/// English-only resources generated from lib/l10n/app_en.arb.
/// Available outside the widget tree for validation and sample app content.
final AppMessages messages = AppMessagesEn();

String describeFailure(Object error) => switch (error) {
  FormatException() => error.message,
  StateError() => error.message.toString(),
  _ => error.toString(),
};
