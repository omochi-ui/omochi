import '../messages.dart';

import 'dart:convert';

import '../models.dart';

bool validNodeId(String value) => RegExp(r'^[a-f0-9]{64}$').hasMatch(value);

class GatewayAddress {
  GatewayAddress({required this.id, required this.ips, required this.relays}) {
    if (!validNodeId(id) || ips.length + relays.length > 64) {
      throw const FormatException('Invalid gateway address');
    }
    for (final relay in relays) {
      final uri = Uri.tryParse(relay);
      if (uri == null ||
          !['https', 'http'].contains(uri.scheme) ||
          uri.host.isEmpty ||
          uri.userInfo.isNotEmpty) {
        throw const FormatException('Invalid relay URL');
      }
    }
    if (ips.any((ip) => ip.length > 256)) {
      throw const FormatException('Invalid direct address');
    }
  }
  factory GatewayAddress.fromJson(Json value) {
    final addresses = value['addrs'] as List;
    return GatewayAddress(
      id: value['id'] as String,
      ips: [
        for (final a in addresses)
          if (a['Ip'] != null) a['Ip'] as String,
      ],
      relays: [
        for (final a in addresses)
          if (a['Relay'] != null) a['Relay'] as String,
      ],
    );
  }
  final String id;
  final List<String> ips, relays;
  Json toJson() => {
    'id': id,
    'addrs': [
      for (final ip in ips) {'Ip': ip},
      for (final r in relays) {'Relay': r},
    ],
  };
}

class GatewayInvitation {
  GatewayInvitation._(this.address, this.secret, this.expires);
  factory GatewayInvitation.parse(String input, {DateTime? now}) {
    if (utf8.encode(input).length > 16384) {
      throw FormatException(messages.errorInvalidInvitation);
    }
    var link = input.trim();
    final web = Uri.tryParse(link);
    if (web != null &&
        web.scheme == 'https' &&
        web.userInfo.isEmpty &&
        !web.hasQuery &&
        web.fragment.startsWith('g:')) {
      link = 'omochi://invite/${web.fragment.substring(2)}';
    }
    const prefix = 'omochi://invite/';
    // Do not include the invitation or secret in exceptions/logs.
    try {
      if (utf8.encode(link).length > 16384 || !link.startsWith(prefix)) {
        throw const FormatException();
      }
      final payload = link.substring(prefix.length);
      if (!RegExp(r'^[A-Za-z0-9_-]+$').hasMatch(payload)) {
        throw const FormatException();
      }
      final value = jsonDecode(
        utf8.decode(base64Url.decode(base64Url.normalize(payload))),
      ) as Json;
      if (value.keys.toSet().difference({
        'version',
        'address',
        'secret',
        'expires',
      }).isNotEmpty) {
        throw const FormatException();
      }
      final expires = value['expires'] as int;
      final secret = value['secret'] as String;
      if (value['version'] != 4 ||
          !RegExp(r'^[a-fA-F0-9]{64}$').hasMatch(secret) ||
          expires <= (now ?? DateTime.now()).millisecondsSinceEpoch ~/ 1000) {
        throw const FormatException();
      }
      return GatewayInvitation._(
        GatewayAddress.fromJson(value['address'] as Json),
        secret,
        expires,
      );
    } catch (_) {
      throw FormatException(messages.errorInvalidInvitation);
    }
  }
  final GatewayAddress address;
  final String secret;
  final int expires;
}

class GatewayPeer {
  const GatewayPeer(this.address, this.name);
  factory GatewayPeer.fromJson(Json value) => GatewayPeer(
    GatewayAddress.fromJson(value['address'] as Json),
    value['name'] as String,
  );
  final GatewayAddress address;
  final String name;
  String get id => address.id;
  Json toJson() => {'address': address.toJson(), 'name': name};
}
