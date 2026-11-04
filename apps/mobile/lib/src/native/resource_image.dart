import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:genui/genui.dart';
import 'package:flutter_markdown_plus/flutter_markdown_plus.dart';

import '../action_input.dart';
import '../messages.dart';
import 'button.dart';
import 'client_api.dart';
import 'column.dart';
import 'image_budget.dart';
import 'modal.dart';
import 'row.dart';

Catalog nativeCatalog(
  GatewayImageRequest Function(String, String) read, {
  required bool Function() useHub,
  required NativeModalRegistry modals,
}) => BasicCatalogItems.asCatalog().copyWith(
  newFunctions: [const ActionInputFunction()],
  newItems: [
    nativeButton,
    nativeColumn,
    nativeRow,
    modals.catalogItem,
    CatalogItem(
      name: BasicCatalogItems.text.name,
      dataSchema: BasicCatalogItems.text.dataSchema,
      widgetBuilder: (item) {
        if (!useHub()) return BasicCatalogItems.text.widgetBuilder(item);
        final data = item.data as Map;
        return BoundString(
          dataContext: item.dataContext,
          value: data['text'] as Object,
          builder: (context, text) {
            final theme = Theme.of(context);
            final style = switch (data['variant']) {
              'h1' => theme.textTheme.headlineLarge,
              'h2' => theme.textTheme.headlineMedium,
              'h3' => theme.textTheme.headlineSmall,
              'h4' => theme.textTheme.titleLarge,
              'h5' => theme.textTheme.titleMedium,
              'caption' => theme.textTheme.bodySmall,
              _ => DefaultTextStyle.of(context).style,
            };
            final padding = switch (data['variant']) {
              'h1' => 20.0,
              'h2' => 16.0,
              'h3' => 12.0,
              'h4' => 8.0,
              'h5' => 4.0,
              _ => 0.0,
            };
            return Padding(
              padding: EdgeInsets.symmetric(vertical: padding),
              child: MarkdownBody(
                data: text ?? '',
                styleSheet: MarkdownStyleSheet.fromTheme(theme)
                    .copyWith(p: style),
                imageBuilder: (uri, title, alt) => _ResourceImage(
                  key: ValueKey('${item.surfaceId}/$uri'),
                  load: () => read(item.surfaceId, uri.toString()),
                  description: alt ?? title ?? messagesFor(context).appImage,
                  fit: BoxFit.contain,
                ),
              ),
            );
          },
        );
      },
    ),
    CatalogItem(
      name: BasicCatalogItems.image.name,
      dataSchema: BasicCatalogItems.image.dataSchema,
      widgetBuilder: (item) {
        final data = item.data as Map;
        return BoundString(
          dataContext: item.dataContext,
          value: data['url'] as Object,
          builder: (context, url) {
            if (!useHub() &&
                (url == null || !url.startsWith('omochi://resources/'))) {
              return BasicCatalogItems.image.widgetBuilder(item);
            }
            if (url == null) return const SizedBox.shrink();
            // item.surfaceId is a renderer-local generation ID. Capture it here;
            // neither the URI nor later action payload chooses the service route.
            final surface = item.surfaceId;
            Widget child = _ResourceImage(
              key: ValueKey('$surface/$url'),
              load: () => read(surface, url),
              fit:
                  BoxFit.values
                      .where((f) => f.name == data['fit'])
                      .firstOrNull ??
                  BoxFit.contain,
            );
            if (data['variant'] == 'avatar') {
              child = ClipOval(child: child);
            }
            if (data['variant'] == 'header') {
              child = SizedBox(width: double.infinity, child: child);
            } else {
              final size = switch (data['variant']) {
                'icon' || 'avatar' => 32.0,
                'smallFeature' => 50.0,
                'largeFeature' => 400.0,
                _ => 150.0,
              };
              child = SizedBox(width: size, height: size, child: child);
            }
            final image = child;
            return BoundString(
              dataContext: item.dataContext,
              value: data['description'] ?? messagesFor(context).appImage,
              builder: (context, label) => Semantics(
                label: label == null || label.isEmpty
                    ? messagesFor(context).appImage
                    : label,
                image: true,
                child: image,
              ),
            );
          },
        );
      },
    ),
  ],
);

class _ResourceImage extends StatefulWidget {
  const _ResourceImage({
    super.key,
    required this.load,
    this.description,
    required this.fit,
  });
  final GatewayImageRequest Function() load;
  final String? description;
  final BoxFit fit;
  @override
  State<_ResourceImage> createState() => _ResourceImageState();
}

class _ResourceImageState extends State<_ResourceImage> {
  GatewayImageRequest? _request;
  NativeDecodedImage? _image;
  int _generation = 0;
  late Future<NativeDecodedImage> _future = _load();
  Future<NativeDecodedImage> _load() async {
    final generation = ++_generation;
    _request?.cancel();
    _image?.dispose();
    _image = null;
    final budget = NativeImageBudget.shared;
    final finishLoad = budget.acquireLoad();
    ui.ImmutableBuffer? buffer;
    ui.ImageDescriptor? descriptor;
    ui.Codec? codec;
    void Function()? releasePixels;
    try {
      final request = widget.load();
      _request = request;
      final bytes = await request.bytes;
      if (!mounted || generation != _generation) {
        throw StateError('Image request cancelled');
      }
      if (bytes.length > NativeImageBudget.maxEncodedBytes) {
        throw StateError('Image exceeds capacity');
      }
      buffer = await ui.ImmutableBuffer.fromUint8List(bytes);
      descriptor = await ui.ImageDescriptor.encoded(buffer);
      releasePixels = budget.reservePixels(descriptor.width, descriptor.height);
      codec = await descriptor.instantiateCodec();
      // Render one frame. Animated codecs otherwise retain additional unbounded
      // frames in Flutter's global image cache. Video has its own component.
      final frame = await codec.getNextFrame();
      final result = NativeDecodedImage(frame.image, releasePixels);
      releasePixels = null;
      if (!mounted || generation != _generation) {
        result.dispose();
        throw StateError('Image request cancelled');
      }
      _image = result;
      return result;
    } finally {
      codec?.dispose();
      descriptor?.dispose();
      buffer?.dispose();
      releasePixels?.call();
      finishLoad();
    }
  }

  @override
  void dispose() {
    _generation++;
    _request?.cancel();
    _image?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<NativeDecodedImage>(
    future: _future,
    builder: (context, snapshot) {
      if (snapshot.connectionState == ConnectionState.done &&
          snapshot.hasError) {
        return TextButton.icon(
          onPressed: () {
            final retry = _load();
            setState(() {
              _future = retry;
            });
          },
          icon: const Icon(Icons.broken_image_outlined),
          label: Text(messagesFor(context).retryImage),
        );
      }
      if (snapshot.connectionState != ConnectionState.done ||
          !snapshot.hasData) {
        return const SizedBox(
          height: 80,
          child: Center(child: CircularProgressIndicator()),
        );
      }
      final child = RawImage(image: snapshot.data!.image, fit: widget.fit);
      if (widget.description == null) return child;
      return Semantics(label: widget.description, image: true, child: child);
    },
  );
}
