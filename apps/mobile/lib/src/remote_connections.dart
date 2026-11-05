import 'dart:async';

import 'package:flutter/material.dart';

import 'app.dart';
import 'messages.dart';
import 'remote/controller.dart';
import 'remote/models.dart';

class RemoteConnections extends StatefulWidget {
  const RemoteConnections({
    super.key,
    this.invitation,
    this.restore = false,
    this.links = const Stream<Uri>.empty(),
    this.controller,
  });
  final String? invitation;
  final bool restore;
  final Stream<Uri> links;
  final RemoteConnectionsController? controller;

  @override
  State<RemoteConnections> createState() => _RemoteConnectionsState();
}

class _RemoteConnectionsState extends State<RemoteConnections> {
  late final controller =
      widget.controller ?? RemoteConnectionsController.native();
  bool adding = false;

  @override
  void initState() {
    super.initState();
    controller.addListener(changed);
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) unawaited(initialize());
    });
  }

  void changed() {
    if (mounted) setState(() {});
  }

  @override
  void dispose() {
    controller.removeListener(changed);
    if (widget.controller == null) controller.dispose();
    super.dispose();
  }

  Future<void> initialize() => observe(() async {
    await controller.load();
    if (!mounted) return;
    if (widget.invitation != null) {
      await add(widget.invitation);
    } else if (widget.restore) {
      final source = await controller.selectedSource();
      if (source != null && mounted) await open(source);
    }
  });

  // Controller state carries failures; event handlers still observe the Future.
  Future<void> observe(Future<void> Function() work) async {
    try {
      await work();
    } catch (_) {}
  }

  Future<void> add([String? initial]) async {
    if (adding || controller.busy) return;
    setState(() => adding = true);
    try {
      final text = TextEditingController(text: initial);
      final input = await showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
          title: Text(messagesFor(context).remoteConnectTitle),
          content: TextField(
            controller: text,
            autofocus: true,
            decoration: InputDecoration(
              labelText: messagesFor(context).remoteConnectInvitation,
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
      // The closing dialog animation still reads its text controller.
      await Future<void>.delayed(const Duration(milliseconds: 300));
      text.dispose();
      if (input == null || !mounted) return;
      await observe(() async {
        final source = await controller.parse(input);
        if (!mounted) return;
        final save = await showDialog<bool>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(
              source.mode == RemoteAccessMode.owner
                  ? messagesFor(context).remoteConnectOwner
                  : messagesFor(context).remoteConnectShared,
            ),
            content: Text(
              '${source.hubId}\n\n${source.mode == RemoteAccessMode.owner ? messagesFor(context).remoteConnectOwnerExplanation : messagesFor(context).remoteConnectSharedExplanation}',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text(messagesFor(context).cancel),
              ),
              if (source.mode == RemoteAccessMode.public)
                TextButton(
                  onPressed: () => Navigator.pop(context, false),
                  child: Text(messagesFor(context).remoteConnectViewOnce),
                ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: Text(messagesFor(context).remoteConnectSave),
              ),
            ],
          ),
        );
        if (save == null || !mounted) return;
        if (source.mode == RemoteAccessMode.public && !save) {
          await open(source, invitation: input);
        } else {
          await controller.register(input, save: save);
          if (mounted) await open(source);
        }
      });
    } finally {
      if (mounted) setState(() => adding = false);
    }
  }

  Future<void> open(RemoteSource source, {String? invitation}) async {
    await controller.open(
      source,
      invitation: invitation,
      present: (session) async {
        if (!mounted) return;
        await Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => MobileHome(links: widget.links, session: session),
          ),
        );
      },
    );
    if (mounted) await controller.load();
  }

  String accessLabel(BuildContext context, RemoteAccessMode mode) =>
      switch (mode) {
        RemoteAccessMode.owner => messagesFor(context).remoteConnectOwner,
        RemoteAccessMode.invite => messagesFor(context).remoteHubInvite,
        RemoteAccessMode.public => messagesFor(context).remoteHubPublic,
      };

  @override
  Widget build(BuildContext context) {
    final busy = adding || controller.busy;
    return Scaffold(
      appBar: AppBar(title: Text(messagesFor(context).remoteHubConnections)),
      body: ListView(
        children: [
          if (controller.error != null)
            ListTile(title: Text(describeFailure(controller.error!))),
          ListTile(
            title: Text(messagesFor(context).remoteHubThisDevice),
            onTap: busy
                ? null
                : () => observe(() async {
                    await controller.select(null);
                    if (context.mounted) Navigator.pop(context);
                  }),
          ),
          for (final source in controller.records)
            ListTile(
              title: Text(
                '${accessLabel(context, source.mode)} · ${source.hubId}',
              ),
              subtitle: Text(
                '${source.grantId}${source.accepted ? '' : ' · ${messagesFor(context).remoteHubPending}'}',
              ),
              onTap: busy ? null : () => observe(() => open(source)),
              trailing: IconButton(
                tooltip: messagesFor(context).forget,
                icon: const Icon(Icons.delete_outline),
                onPressed: busy
                    ? null
                    : () => observe(() => controller.forget(source.id)),
              ),
            ),
          ListTile(
            leading: const Icon(Icons.add_link),
            title: Text(messagesFor(context).remoteHubAdd),
            onTap: busy ? null : () => add(),
          ),
        ],
      ),
    );
  }
}
