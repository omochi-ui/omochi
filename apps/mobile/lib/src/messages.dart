import 'package:flutter/widgets.dart';

import '../l10n/generated/app_messages.dart';

/// Follow the same system locale resolution as MaterialApp outside the tree.
AppMessages get messages => lookupAppMessages(
  basicLocaleListResolution(
    WidgetsBinding.instance.platformDispatcher.locales,
    AppMessages.supportedLocales,
  ),
);

/// Resolve through Localizations so widgets rebuild when the locale changes.
AppMessages messagesFor(BuildContext context) =>
    Localizations.of<AppMessages>(context, AppMessages) ??
    lookupAppMessages(
      basicLocaleListResolution([
        Localizations.localeOf(context),
      ], AppMessages.supportedLocales),
    );

String describeFailure(Object error) => switch (error) {
  FormatException() => error.message,
  StateError() => error.message.toString(),
  _ => error.toString(),
};
