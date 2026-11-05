import 'package:flutter/material.dart';
import 'package:genui/genui.dart';

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
          : SingleChildScrollView(
              child: SingleChildScrollView(
                scrollDirection: Axis.horizontal,
                child: SizedBox(
                  width: 960,
                  height: controller.board.rows * 80.0,
                  child: Stack(
                    children: [
                      for (final panel in controller.board.panels)
                        Positioned(
                          left: panel.x * 80.0,
                          top: panel.y * 80.0,
                          width: panel.w * 80.0,
                          height: panel.h * 80.0,
                          child: Card(
                            child: Column(
                              children: [
                                Text(panel.title),
                                Expanded(child: surface(panel.reference)),
                              ],
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
    ),
  );
  Widget surface(SurfaceReference reference) {
    if (!widget.session.surfaceIds.contains(reference.id)) {
      return const Center(child: Icon(Icons.cloud_off_outlined));
    }
    final context = widget.session.contextForSurface(reference.id);
    return IgnorePointer(
      ignoring: !widget.repository.interact,
      child: Surface(key: ValueKey(context.surfaceId), surfaceContext: context),
    );
  }
}
