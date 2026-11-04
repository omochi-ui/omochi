typedef Json = Map<String, dynamic>;

class HubService {
  HubService.fromJson(Json value)
    : id = value['id'] as String,
      name = value['name'] as String,
      status = value['status'] as String,
      detail = (value['detail'] ?? value['error']) as String?,
      nodeId = value['node']?['id'] as String?,
      nodeName = value['node']?['name'] as String?;
  final String id, name, status;
  final String? nodeId, nodeName, detail;
}

String channelOf(String id) =>
    id.contains('/') ? id.substring(0, id.indexOf('/')) : '';
