// Generated from lib/l10n/app_en.arb by flutter gen-l10n. Do not edit.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:intl/intl.dart' as intl;

import 'app_messages_en.dart';

// ignore_for_file: type=lint

/// Callers can lookup localized strings with an instance of AppMessages
/// returned by `AppMessages.of(context)`.
///
/// Applications need to include `AppMessages.delegate()` in their app's
/// `localizationDelegates` list, and the locales they support in the app's
/// `supportedLocales` list. For example:
///
/// ```dart
/// import 'generated/app_messages.dart';
///
/// return MaterialApp(
///   localizationsDelegates: AppMessages.localizationsDelegates,
///   supportedLocales: AppMessages.supportedLocales,
///   home: MyApplicationHome(),
/// );
/// ```
///
/// ## Update pubspec.yaml
///
/// Please make sure to update your pubspec.yaml to include the following
/// packages:
///
/// ```yaml
/// dependencies:
///   # Internationalization support.
///   flutter_localizations:
///     sdk: flutter
///   intl: any # Use the pinned version from flutter_localizations
///
///   # Rest of dependencies
/// ```
///
/// ## iOS Applications
///
/// iOS applications define key application metadata, including supported
/// locales, in an Info.plist file that is built into the application bundle.
/// To configure the locales supported by your app, you’ll need to edit this
/// file.
///
/// First, open your project’s ios/Runner.xcworkspace Xcode workspace file.
/// Then, in the Project Navigator, open the Info.plist file under the Runner
/// project’s Runner folder.
///
/// Next, select the Information Property List item, select Add Item from the
/// Editor menu, then select Localizations from the pop-up menu.
///
/// Select and expand the newly-created Localizations item then, for each
/// locale your application supports, add a new item and select the locale
/// you wish to add from the pop-up menu in the Value field. This list should
/// be consistent with the languages listed in the AppMessages.supportedLocales
/// property.
abstract class AppMessages {
  AppMessages(String locale)
    : localeName = intl.Intl.canonicalizedLocale(locale.toString());

  final String localeName;

  static AppMessages of(BuildContext context) {
    return Localizations.of<AppMessages>(context, AppMessages)!;
  }

  static const LocalizationsDelegate<AppMessages> delegate =
      _AppMessagesDelegate();

  /// A list of this localizations delegate along with the default localizations
  /// delegates.
  ///
  /// Returns a list of localizations delegates containing this delegate along with
  /// GlobalMaterialLocalizations.delegate, GlobalCupertinoLocalizations.delegate,
  /// and GlobalWidgetsLocalizations.delegate.
  ///
  /// Additional delegates can be added by appending to this list in
  /// MaterialApp. This list does not have to be used at all if a custom list
  /// of delegates is preferred or required.
  static const List<LocalizationsDelegate<dynamic>> localizationsDelegates =
      <LocalizationsDelegate<dynamic>>[
        delegate,
        GlobalMaterialLocalizations.delegate,
        GlobalCupertinoLocalizations.delegate,
        GlobalWidgetsLocalizations.delegate,
      ];

  /// A list of this localizations delegate's supported locales.
  static const List<Locale> supportedLocales = <Locale>[Locale('en')];

  /// No description provided for @apps.
  ///
  /// In en, this message translates to:
  /// **'Apps'**
  String get apps;

  /// No description provided for @cancel.
  ///
  /// In en, this message translates to:
  /// **'Cancel'**
  String get cancel;

  /// No description provided for @internetRelays.
  ///
  /// In en, this message translates to:
  /// **'Internet relays'**
  String get internetRelays;

  /// No description provided for @deviceIdentity.
  ///
  /// In en, this message translates to:
  /// **'This device'**
  String get deviceIdentity;

  /// No description provided for @gatewayInvitation.
  ///
  /// In en, this message translates to:
  /// **'Gateway invitation'**
  String get gatewayInvitation;

  /// No description provided for @invitationExample.
  ///
  /// In en, this message translates to:
  /// **'omochi://invite/…'**
  String get invitationExample;

  /// No description provided for @forget.
  ///
  /// In en, this message translates to:
  /// **'Forget'**
  String get forget;

  /// No description provided for @appImage.
  ///
  /// In en, this message translates to:
  /// **'App image'**
  String get appImage;

  /// No description provided for @retryImage.
  ///
  /// In en, this message translates to:
  /// **'Retry image'**
  String get retryImage;

  /// No description provided for @errorImageNeedsNativeHub.
  ///
  /// In en, this message translates to:
  /// **'Connect the built-in Hub to load this image'**
  String get errorImageNeedsNativeHub;

  /// No description provided for @errorOversizedMessage.
  ///
  /// In en, this message translates to:
  /// **'A2UI message exceeds 4 MiB'**
  String get errorOversizedMessage;

  /// No description provided for @errorInvalidServerMessage.
  ///
  /// In en, this message translates to:
  /// **'Expected one A2UI v0.9 server message'**
  String get errorInvalidServerMessage;

  /// No description provided for @errorDisconnectedAction.
  ///
  /// In en, this message translates to:
  /// **'Disconnected; action was not sent.'**
  String get errorDisconnectedAction;

  /// No description provided for @errorStoppedHub.
  ///
  /// In en, this message translates to:
  /// **'Built-in Hub is stopped'**
  String get errorStoppedHub;

  /// No description provided for @errorInvalidInvitation.
  ///
  /// In en, this message translates to:
  /// **'Invalid or expired Gateway invitation. Create a new invitation on the Gateway.'**
  String get errorInvalidInvitation;

  /// No description provided for @demoNotes.
  ///
  /// In en, this message translates to:
  /// **'Field notes'**
  String get demoNotes;

  /// No description provided for @demoPlayground.
  ///
  /// In en, this message translates to:
  /// **'Playground'**
  String get demoPlayground;

  /// No description provided for @demoDevice.
  ///
  /// In en, this message translates to:
  /// **'This device · demo'**
  String get demoDevice;

  /// No description provided for @demoAddOne.
  ///
  /// In en, this message translates to:
  /// **'Add one'**
  String get demoAddOne;

  /// No description provided for @demoTask.
  ///
  /// In en, this message translates to:
  /// **'What are you working on?'**
  String get demoTask;

  /// No description provided for @demoSetIntention.
  ///
  /// In en, this message translates to:
  /// **'Set intention'**
  String get demoSetIntention;

  /// No description provided for @demoSampleTask.
  ///
  /// In en, this message translates to:
  /// **'Explore Omochi'**
  String get demoSampleTask;

  /// No description provided for @demoNote.
  ///
  /// In en, this message translates to:
  /// **'Example text from an app.'**
  String get demoNote;

  /// Failure with the underlying error detail.
  ///
  /// In en, this message translates to:
  /// **'Action failed: {error}'**
  String errorActionFailed(String error);

  /// Failure with the underlying error detail.
  ///
  /// In en, this message translates to:
  /// **'A2UI: {error}'**
  String errorRenderingFailed(String error);

  /// Failure with the underlying error detail.
  ///
  /// In en, this message translates to:
  /// **'Built-in Hub: {error}'**
  String errorHubFailed(String error);

  /// Failure with the underlying error detail.
  ///
  /// In en, this message translates to:
  /// **'Pairing failed: {error}'**
  String errorPairingFailed(String error);

  /// Demo counter value.
  ///
  /// In en, this message translates to:
  /// **'{count, plural, =1{1 moment} other{{count} moments}}'**
  String demoMoments(int count);

  /// No description provided for @addGateway.
  ///
  /// In en, this message translates to:
  /// **'Add gateway'**
  String get addGateway;

  /// No description provided for @addOnDevice.
  ///
  /// In en, this message translates to:
  /// **'Add this gateway to the Hub on this device.'**
  String get addOnDevice;

  /// No description provided for @addOnDeviceButton.
  ///
  /// In en, this message translates to:
  /// **'Add to this device'**
  String get addOnDeviceButton;

  /// No description provided for @expiresAt.
  ///
  /// In en, this message translates to:
  /// **'Expires: {time}'**
  String expiresAt(String time);

  /// No description provided for @invitationPending.
  ///
  /// In en, this message translates to:
  /// **'Another invitation or operation is pending.'**
  String get invitationPending;

  /// No description provided for @gatewayAdded.
  ///
  /// In en, this message translates to:
  /// **'Gateway added.'**
  String get gatewayAdded;

  /// No description provided for @continueButton.
  ///
  /// In en, this message translates to:
  /// **'Continue'**
  String get continueButton;

  /// No description provided for @startWithInvitation.
  ///
  /// In en, this message translates to:
  /// **'Open a gateway invitation or add one in Connections.'**
  String get startWithInvitation;

  /// No description provided for @waitingForApps.
  ///
  /// In en, this message translates to:
  /// **'Waiting for gateway apps…'**
  String get waitingForApps;

  /// No description provided for @forgetGatewayTitle.
  ///
  /// In en, this message translates to:
  /// **'Forget gateway?'**
  String get forgetGatewayTitle;

  /// No description provided for @dismiss.
  ///
  /// In en, this message translates to:
  /// **'Dismiss'**
  String get dismiss;

  /// No description provided for @connections.
  ///
  /// In en, this message translates to:
  /// **'Connections'**
  String get connections;

  /// No description provided for @retry.
  ///
  /// In en, this message translates to:
  /// **'Retry'**
  String get retry;

  /// No description provided for @forgetLocalExplanation.
  ///
  /// In en, this message translates to:
  /// **'Remove the registration from this device. Revoking this device on the gateway is a separate operation.'**
  String get forgetLocalExplanation;
}

class _AppMessagesDelegate extends LocalizationsDelegate<AppMessages> {
  const _AppMessagesDelegate();

  @override
  Future<AppMessages> load(Locale locale) {
    return SynchronousFuture<AppMessages>(lookupAppMessages(locale));
  }

  @override
  bool isSupported(Locale locale) =>
      <String>['en'].contains(locale.languageCode);

  @override
  bool shouldReload(_AppMessagesDelegate old) => false;
}

AppMessages lookupAppMessages(Locale locale) {
  // Lookup logic when only language code is specified.
  switch (locale.languageCode) {
    case 'en':
      return AppMessagesEn();
  }

  throw FlutterError(
    'AppMessages.delegate failed to load unsupported locale "$locale". This is likely '
    'an issue with the localizations generation tool. Please file an issue '
    'on GitHub with a reproducible sample app and the gen-l10n configuration '
    'that was used.',
  );
}
