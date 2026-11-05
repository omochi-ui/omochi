import 'dart:async';

import 'package:flutter/material.dart';

import 'app.dart';
import 'messages.dart';
import 'hub_session.dart';
import 'remote/repository.dart';
import 'remote/controller.dart';
import 'remote/models.dart';
import 'remote/invitation_routing.dart';

class RemoteConnections extends StatefulWidget {
  const RemoteConnections({
    super.key,
    this.invitation,
    this.restore = false,
    this.invitationRouting,
    this.controller,
    this.repository,
  });
  final String? invitation;
  final bool restore;
  final InvitationRouting? invitationRouting;
  final RemoteConnectionsController? controller;
  final RemoteRepository? repository;

  @override
  State<RemoteConnections> createState() => _RemoteConnectionsState();
}

class _RemoteConnectionsState extends State<RemoteConnections> {
  late final controller =
      widget.controller ??
      RemoteConnectionsController(
        widget.repository ?? NativeRemoteRepository.mobile(),
      );
  bool adding = false;
  int addGeneration = 0;
  InvitationRoute? invitationRoute;
  Future<void>? opening;
  HubSession? activeSession;
  bool closingRemote = false;

  @override
  void initState() {
    super.initState();
    controller.addListener(changed);
    invitationRoute = widget.invitationRouting?.enter(
      handleInvitation,
      mayLeave: closeRemote,
    );
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
    final route = invitationRoute;
    if (route != null) widget.invitationRouting?.leave(route);
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
    final generation = ++addGeneration;
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
              if (source.mode == RemoteAccessMode.share)
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
        if (source.mode == RemoteAccessMode.share && !save) {
          await open(source, invitation: input);
        } else {
          await controller.register(input, save: save);
          if (mounted) await open(source);
        }
      });
    } finally {
      if (mounted && generation == addGeneration) {
        setState(() => adding = false);
      }
    }
  }

  Future<bool> closeRemote() async {
    final opened = opening;
    if (opened == null) return true;
    closingRemote = true;
    try {
      final route = ModalRoute.of(context);
      Navigator.of(context).popUntil((candidate) => candidate == route);
      // The local Hub must stay suspended until the remote identity is released.
      await opened;
      return mounted;
    } finally {
      closingRemote = false;
    }
  }

  Future<void> handleInvitation(String input) async {
    final session = activeSession;
    if (session != null &&
        !(await (session.remoteLeaveGuard?.call() ?? Future.value(true)))) {
      return;
    }
    if (!mounted || !await closeRemote() || !mounted) return;
    await add(input);
  }

  Future<void> open(RemoteSource source, {String? invitation}) async {
    if (adding) setState(() => adding = false);
    final opened = controller.open(
      source,
      invitation: invitation,
      present: (session) async {
        if (!mounted || closingRemote) return;
        activeSession = session;
        try {
          await Navigator.of(context).push(
            MaterialPageRoute<void>(
              builder: (_) => MobileHome(
                links: const Stream<Uri>.empty(),
                session: session,
                invitationRouting: widget.invitationRouting,
                remoteRepository: controller.repository,
                onHubInvitation: handleInvitation,
              ),
            ),
          );
        } finally {
          activeSession = null;
        }
      },
    );
    opening = opened;
    try {
      await opened;
    } finally {
      opening = null;
    }
  }

  String accessLabel(BuildContext context, RemoteAccessMode mode) =>
      switch (mode) {
        RemoteAccessMode.owner => messagesFor(context).remoteConnectOwner,
        RemoteAccessMode.share => messagesFor(context).remoteHubPublic,
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
