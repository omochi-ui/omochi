import 'package:flutter/material.dart';

import 'hub_session.dart';
import 'messages.dart';
import 'remote/board_models.dart';
import 'remote/boards_controller.dart';
import 'remote/boards_repository.dart';
import 'remote/board_editor.dart';
import 'remote/board_view.dart';
import 'remote/hub_shares.dart';

export 'remote/board_editor.dart' show BoardEditor;
export 'remote/board_view.dart' show BoardView;
export 'remote/hub_shares.dart' show HubShares;

enum _BoardMenu { presets, shares, connections, sharing }

class RemoteBoards extends StatefulWidget {
  const RemoteBoards({
    super.key,
    required this.repository,
    required this.session,
  });
  final BoardsRepository repository;
  final HubSession session;
  @override
  State<RemoteBoards> createState() => _RemoteBoardsState();
}

class _RemoteBoardsState extends State<RemoteBoards> {
  late final controller = BoardsController(widget.repository)..start();
  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  Future<void> edit(RemoteBoard board, {bool create = false}) async {
    await Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => BoardEditor(
          repository: widget.repository,
          session: widget.session,
          board: board,
          create: create,
        ),
      ),
    );
    await controller.refresh();
  }

  Future<void> menu(_BoardMenu item) async {
    switch (item) {
      case _BoardMenu.presets:
        final presets = await controller.presets();
        if (!mounted || presets == null) return;
        final preset = await showDialog<RemoteBoard>(
          context: context,
          builder: (context) => SimpleDialog(
            title: Text(messagesFor(context).remotePresets),
            children: [
              for (final preset in presets)
                SimpleDialogOption(
                  onPressed: () => Navigator.pop(context, preset),
                  child: Text(preset.name),
                ),
            ],
          ),
        );
        if (preset != null && mounted) await edit(preset, create: true);
      case _BoardMenu.shares:
        await Navigator.of(context).push(
          MaterialPageRoute<void>(
            builder: (_) => HubShares(
              repository: widget.repository,
              boards: controller.boards,
            ),
          ),
        );
      case _BoardMenu.connections:
        final connections = await controller.connections();
        if (!mounted || connections == null) return;
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(messagesFor(context).remoteConnections),
            content: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  for (final connection in connections)
                    ListTile(
                      title: Text(connection.name),
                      subtitle: Text(connection.status),
                    ),
                ],
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text(MaterialLocalizations.of(context).okButtonLabel),
              ),
            ],
          ),
        );
      case _BoardMenu.sharing:
        final configuration = await controller.sharing();
        if (!mounted || configuration == null) return;
        await showDialog<void>(
          context: context,
          builder: (context) => AlertDialog(
            title: Text(messagesFor(context).remoteSharingConfiguration),
            content: SingleChildScrollView(
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    configuration.enabled
                        ? messagesFor(context).remoteSharingEnabled
                        : messagesFor(context).remoteSharingDisabled,
                  ),
                  if (configuration.hubId != null)
                    SelectableText(configuration.hubId!),
                  for (final relay in configuration.relayUrls) Text(relay),
                ],
              ),
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context),
                child: Text(MaterialLocalizations.of(context).okButtonLabel),
              ),
            ],
          ),
        );
    }
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: controller,
    builder: (context, _) => Scaffold(
      appBar: AppBar(
        title: Text(messagesFor(context).remoteBoards),
        actions: [
          if (widget.repository.owner)
            PopupMenuButton<_BoardMenu>(
              onSelected: menu,
              itemBuilder: (context) => [
                PopupMenuItem(
                  value: _BoardMenu.presets,
                  child: Text(messagesFor(context).remotePresets),
                ),
                PopupMenuItem(
                  value: _BoardMenu.shares,
                  child: Text(messagesFor(context).remoteSharing),
                ),
                PopupMenuItem(
                  value: _BoardMenu.connections,
                  child: Text(messagesFor(context).remoteConnections),
                ),
                PopupMenuItem(
                  value: _BoardMenu.sharing,
                  child: Text(messagesFor(context).remoteSharingConfiguration),
                ),
              ],
            ),
        ],
      ),
      body: ListView(
        children: [
          if (controller.error case final failure?)
            ListTile(title: Text(describeFailure(failure))),
          for (final board in controller.boards)
            ListTile(
              title: Text(board.name),
              subtitle: Text(
                messagesFor(context).remoteRevision(board.revision ?? 0),
              ),
              onTap: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => BoardView(
                    board: board,
                    session: widget.session,
                    repository: widget.repository,
                  ),
                ),
              ),
              trailing: widget.repository.editLayout
                  ? IconButton(
                      icon: const Icon(Icons.edit_outlined),
                      onPressed: () => edit(board),
                    )
                  : null,
            ),
        ],
      ),
      floatingActionButton: widget.repository.owner
          ? FloatingActionButton(
              onPressed: () => edit(RemoteBoard.empty(), create: true),
              child: const Icon(Icons.add),
            )
          : null,
    ),
  );
}
