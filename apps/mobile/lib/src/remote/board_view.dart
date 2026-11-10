import 'package:flutter/material.dart';

import 'board_canvas.dart';

import '../hub_session.dart';
import '../messages.dart';
import 'board_models.dart';
import 'boards_controller.dart';
import 'boards_repository.dart';

class BoardView extends StatefulWidget {
  const BoardView({
    super.key,
    required this.board,
    required this.session,
    required this.repository,
  });
  final RemoteBoard board;
  final HubSession session;
  final BoardsRepository repository;
  @override
  State<BoardView> createState() => _BoardViewState();
}

class _BoardViewState extends State<BoardView> {
  late final controller = BoardViewController(widget.repository, widget.board)
    ..start();
  @override
  void dispose() {
    controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => ListenableBuilder(
    listenable: Listenable.merge([controller, widget.session]),
    builder: (context, _) => Scaffold(
      appBar: AppBar(title: Text(controller.board.name)),
      body: controller.error != null
          ? Center(child: Text(describeFailure(controller.error!)))
          : BoardCanvas(
              panels: controller.board.panels,
              session: widget.session,
              interact: widget.repository.interact,
            ),
    ),
  );
}
