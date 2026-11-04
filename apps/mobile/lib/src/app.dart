import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/material.dart';
import 'package:genui/genui.dart';

import '../l10n/generated/app_messages.dart';
import 'hub_session.dart';
import 'messages.dart';
import 'models.dart';
import 'native/protocol.dart';

class MobileApp extends StatelessWidget {
  const MobileApp({super.key, required this.links, this.session});
  final Stream<Uri> links;
  final HubSession? session;
  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Omochi',
    localizationsDelegates: AppMessages.localizationsDelegates,
    supportedLocales: AppMessages.supportedLocales,
    theme: ThemeData(
      colorSchemeSeed: const Color(0xff6450aa),
      useMaterial3: true,
    ),
    home: MobileHome(links: links, session: session),
  );
}

class MobileHome extends StatefulWidget {
  const MobileHome({super.key, required this.links, this.session});
  final HubSession? session;
  final Stream<Uri> links;
  @override
  State<MobileHome> createState() => _MobileHomeState();
}

class _MobileHomeState extends State<MobileHome> with WidgetsBindingObserver {
  late final hub = widget.session ?? HubSession();
  StreamSubscription<Uri>? subscription;
  Future<void> lifecycle = Future.value();
  late Future<void> ready;
  String? pending;
  String? selected;
  int tab = 0;
  bool busy = false;
  bool lifecycleSuspended = false;
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    hub.addListener(changed);
    ready = hub.startNative();
    unawaited(
      ready.catchError((Object e) {
        if (mounted) notice(describeFailure(e));
      }),
    );
    subscription = widget.links.listen((uri) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) unawaited(invite(uri.toString()));
      });
    }, onError: (Object e) => notice(describeFailure(e)));
  }

  void changed() {
    if (mounted) setState(() {});
  }

  void notice(String text) {
    if (!mounted) return;
    ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(text)));
  }

  Future<void> run(Future<void> Function() action) async {
    if (busy) return;
    setState(() => busy = true);
    try {
      await ready;
      await action();
    } catch (e) {
      notice(describeFailure(e));
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> invite(String input) async {
    GatewayInvitation invitation;
    try {
      invitation = GatewayInvitation.parse(input);
    } catch (e) {
      notice(describeFailure(e));
      return;
    }
    final key = '${invitation.address.id}/${invitation.secret}';
    if (pending == key) return;
    if (pending != null || busy) {
      notice(messagesFor(context).invitationPending);
      return;
    }
    pending = key;
    try {
      final accepted = await showDialog<bool>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(messagesFor(context).addGateway),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(messagesFor(context).addOnDevice),
                const SizedBox(height: 16),
                SelectableText(invitation.address.id),
                const SizedBox(height: 12),
                Text(
                  messagesFor(context).expiresAt(
                    DateTime.fromMillisecondsSinceEpoch(
                      invitation.expires * 1000,
                    ).toLocal().toString(),
                  ),
                ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context, false),
              child: Text(messagesFor(context).cancel),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(context, true),
              child: Text(messagesFor(context).addOnDeviceButton),
            ),
          ],
        ),
      );
      if (accepted == true && mounted) {
        await run(() async {
          GatewayInvitation.parse(input); // Recheck expiry after confirmation.
          await hub.pairGateway(input);
          if (mounted) {
            setState(() => tab = 0);
            notice(messagesFor(context).gatewayAdded);
          }
        });
      }
    } finally {
      pending = null;
    }
  }

  Future<void> paste() async {
    final text = TextEditingController();
    final result = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(messagesFor(context).gatewayInvitation),
        content: TextField(
          controller: text,
          autofocus: true,
          maxLines: 4,
          decoration: InputDecoration(
            hintText: messagesFor(context).invitationExample,
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: Text(messagesFor(context).cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, text.text),
            child: Text(messagesFor(context).continueButton),
          ),
        ],
      ),
    );
    // Dialog's exit animation may still reference its controller.
    await Future<void>.delayed(const Duration(milliseconds: 300));
    text.dispose();
    if (result != null && mounted) await invite(result);
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed &&
        state != AppLifecycleState.paused &&
        state != AppLifecycleState.detached) {
      return;
    }
    lifecycle = lifecycle
        .then((_) async {
          await ready;
          if (state == AppLifecycleState.resumed) {
            if (!lifecycleSuspended) return;
            await hub.resumeNative();
            lifecycleSuspended = false;
          } else {
            if (lifecycleSuspended) return;
            // A failed suspension may already have stopped rendering/networking.
            lifecycleSuspended = true;
            await hub.suspendNative();
          }
        })
        .catchError((Object e) {
          notice(describeFailure(e));
        });
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    subscription?.cancel();
    hub.removeListener(changed);
    hub.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final ids = hub.surfaceIds;
    final surface = selected != null && ids.contains(selected)
        ? selected
        : null;
    final surfaceContext = surface == null
        ? null
        : hub.contextForSurface(surface);
    return Scaffold(
      appBar: AppBar(
        title: Text(
          tab == 0
              ? messagesFor(context).apps
              : messagesFor(context).connections,
        ),
        actions: [
          if (hub.status == 'Unavailable')
            IconButton(
              tooltip: messagesFor(context).retry,
              onPressed: busy
                  ? null
                  : () {
                      ready = hub.startNative();
                      unawaited(
                        ready.catchError(
                          (Object e) => notice(describeFailure(e)),
                        ),
                      );
                    },
              icon: const Icon(Icons.refresh),
            ),
        ],
      ),
      body: SafeArea(
        child: Column(
          children: [
            if (busy) const LinearProgressIndicator(),
            if (hub.error != null)
              MaterialBanner(
                content: Text(hub.error!),
                actions: [
                  TextButton(
                    onPressed: () => setState(() => hub.error = null),
                    child: Text(messagesFor(context).dismiss),
                  ),
                ],
              ),
            Expanded(
              child: tab == 0
                  ? surface != null
                        ? Padding(
                            padding: const EdgeInsets.all(16),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                TextButton.icon(
                                  onPressed: () =>
                                      setState(() => selected = null),
                                  icon: const Icon(Icons.arrow_back),
                                  label: Text(messagesFor(context).apps),
                                ),
                                if (hub.surfaceErrors[surface] != null)
                                  Text(hub.surfaceErrors[surface]!),
                                Expanded(
                                  child: Surface(
                                    key: ValueKey(
                                      '${hub.rendererVersion}/${surfaceContext!.surfaceId}',
                                    ),
                                    surfaceContext: surfaceContext,
                                  ),
                                ),
                              ],
                            ),
                          )
                        : ListView(
                            padding: const EdgeInsets.all(16),
                            children: [
                              if (hub.services.isEmpty)
                                Padding(
                                  padding: const EdgeInsets.all(24),
                                  child: Text(
                                    hub.gateways.isEmpty
                                        ? messagesFor(context)
                                              .startWithInvitation
                                        : messagesFor(context).waitingForApps,
                                  ),
                                ),
                              for (final service in hub.services)
                                Card(
                                  child: Column(
                                    children: [
                                      ListTile(
                                        title: Text(service.name),
                                        subtitle: Text(
                                          service.detail ?? service.status,
                                        ),
                                      ),
                                      for (final id in ids.where(
                                        (id) => channelOf(id) == service.id,
                                      ))
                                        ListTile(
                                          title: Text(
                                            id.substring(id.indexOf('/') + 1),
                                          ),
                                          trailing: const Icon(
                                            Icons.chevron_right,
                                          ),
                                          onTap: () =>
                                              setState(() => selected = id),
                                        ),
                                    ],
                                  ),
                                ),
                            ],
                          )
                  : ListView(
                      padding: const EdgeInsets.all(16),
                      children: [
                        FilledButton.icon(
                          onPressed: busy ? null : paste,
                          icon: const Icon(Icons.add),
                          label: Text(messagesFor(context).addGateway),
                        ),
                        const SizedBox(height: 16),
                        Text(hub.status),
                        if (hub.nativeIdentity != null)
                          ListTile(
                            title: Text(messagesFor(context).deviceIdentity),
                            subtitle: SelectableText(hub.nativeIdentity!),
                          ),
                        SwitchListTile(
                          title: Text(messagesFor(context).internetRelays),
                          value: hub.nativeOnline,
                          onChanged: busy
                              ? null
                              : (value) =>
                                    run(() => hub.setNativeOnline(value)),
                        ),
                        for (final peer in hub.gateways)
                          Card(
                            child: Column(
                              children: [
                                ListTile(
                                  title: Text(peer.name),
                                  subtitle: Text(
                                    hub.connections
                                        .where(
                                          (c) => c['id'] == peer.address.id,
                                        )
                                        .map((c) => c['status'])
                                        .join(', '),
                                  ),
                                ),
                                OverflowBar(
                                  children: [
                                    TextButton(
                                      onPressed: busy
                                          ? null
                                          : () => hub.retryGateway(
                                              peer.address.id,
                                            ),
                                      child: Text(messagesFor(context).retry),
                                    ),
                                    TextButton(
                                      onPressed: busy
                                          ? null
                                          : () async {
                                              final yes = await showDialog<bool>(
                                                context: context,
                                                builder: (context) =>
                                                    AlertDialog(
                                                      title: Text(
                                                        messagesFor(context)
                                                            .forgetGatewayTitle,
                                                      ),
                                                      content: Text(
                                                        messagesFor(
                                                          context,
                                                        ).forgetLocalExplanation,
                                                      ),
                                                      actions: [
                                                        TextButton(
                                                          onPressed: () =>
                                                              Navigator.pop(
                                                                context,
                                                                false,
                                                              ),
                                                          child: Text(
                                                            messagesFor(context)
                                                                .cancel,
                                                          ),
                                                        ),
                                                        FilledButton(
                                                          onPressed: () =>
                                                              Navigator.pop(
                                                                context,
                                                                true,
                                                              ),
                                                          child: Text(
                                                            messagesFor(context)
                                                                .forget,
                                                          ),
                                                        ),
                                                      ],
                                                    ),
                                              );
                                              if (yes == true) {
                                                await run(
                                                  () => hub.forgetGateway(
                                                    peer.address.id,
                                                  ),
                                                );
                                              }
                                            },
                                      child: Text(messagesFor(context).forget),
                                    ),
                                  ],
                                ),
                              ],
                            ),
                          ),
                      ],
                    ),
            ),
          ],
        ),
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: tab,
        onDestinationSelected: (value) => setState(() => tab = value),
        destinations: [
          NavigationDestination(
            icon: const Icon(Icons.apps),
            label: messagesFor(context).apps,
          ),
          NavigationDestination(
            icon: const Icon(Icons.link),
            label: messagesFor(context).connections,
          ),
        ],
      ),
    );
  }
}

Stream<Uri> mobileLinks() => AppLinks().uriLinkStream;
