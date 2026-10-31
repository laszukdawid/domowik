import 'dart:convert';
import 'dart:typed_data';

import 'account.dart';

/// One HTTP request to the hosted service. A body is JSON text or raw bytes.
class HostedRequest {
  const HostedRequest(
    this.method,
    this.url, {
    this.headers = const {},
    this.text,
    this.bytes,
  });

  final String method;
  final Uri url;
  final Map<String, String> headers;
  final String? text;
  final Uint8List? bytes;
}

/// The service's answer. [header] looks a response header up by name.
class HostedResponse {
  const HostedResponse(this.status, this.body, this.header);

  final int status;
  final Uint8List body;
  final String? Function(String name) header;

  bool get ok => status >= 200 && status <= 299;
  String get text => utf8.decode(body, allowMalformed: true);

  /// The decoded JSON body, or null when it is not JSON.
  Object? get json {
    try {
      return jsonDecode(text);
    } on FormatException {
      return null;
    }
  }

  /// The service's refusal, carrying its status and error `code`.
  AccountException refused() {
    final decoded = json;
    final message = decoded is Map && decoded['message'] is String
        ? decoded['message'] as String
        : 'no details';
    final code = decoded is Map && decoded['code'] is String
        ? decoded['code'] as String
        : null;
    return AccountException(
      'The server refused ($status): $message',
      statusCode: status,
      code: code,
    );
  }
}

/// The hosted plan routes, shared by the browser and desktop clients. A client
/// supplies the transport, its credentials and its lease label; routes,
/// payloads, revision headers and error codes live only here.
abstract class HostedPlanProtocol implements HostedPlanClient {
  ServerEndpoint get server;

  /// The writer lease's label: `browser` for the page, `api` for the app.
  String get leaseMode;

  /// Headers that authenticate [session]; [mutation] marks a write.
  Map<String, String> credentials(
    AccountSession session, {
    required bool mutation,
  });

  /// Sends one request. Network failures throw [AccountException].
  Future<HostedResponse> transport(HostedRequest request);

  /// Sends an authenticated request to `/api/v1/[path]`.
  Future<HostedResponse> request(
    AccountSession session,
    String method,
    String path, {
    Map<String, String>? query,
    Map<String, Object?>? json,
    Uint8List? bytes,
    Map<String, String> headers = const {},
  }) => transport(
    HostedRequest(
      method,
      server.api(path, query),
      headers: {
        ...credentials(session, mutation: method != 'GET'),
        ...headers,
        if (json != null) 'Content-Type': 'application/json',
        if (bytes != null) 'Content-Type': 'application/octet-stream',
      },
      text: json == null ? null : jsonEncode(json),
      bytes: bytes,
    ),
  );

  /// The JSON answer of a request that must succeed.
  Future<Object?> requestJson(
    AccountSession session,
    String method,
    String path, {
    Map<String, String>? query,
    Map<String, Object?>? json,
    Uint8List? bytes,
    Map<String, String> headers = const {},
  }) async {
    final response = await request(
      session,
      method,
      path,
      query: query,
      json: json,
      bytes: bytes,
      headers: headers,
    );
    if (!response.ok) throw response.refused();
    return response.json;
  }

  /// A request that must answer 204 No Content.
  Future<void> requestNoContent(
    AccountSession session,
    String method,
    String path, {
    Map<String, Object?>? json,
  }) async {
    final response = await request(session, method, path, json: json);
    if (response.status != 204) throw response.refused();
  }

  static String planPath(String id) => 'plans/${Uri.encodeComponent(id)}';

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) async {
    final decoded = await requestJson(session, 'GET', 'plans');
    if (decoded is! List) {
      throw const AccountException('The server sent an unexpected plan list');
    }
    return [for (final item in decoded) _plan(item)];
  }

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) async =>
      _plan(
        await requestJson(session, 'POST', 'plans', json: {'title': title}),
      );

  @override
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  ) async => _plan(
    await requestJson(
      session,
      'POST',
      'plans/import',
      query: {'title': title},
      bytes: bytes,
    ),
  );

  @override
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  }) async => _lease(
    await requestJson(
      session,
      'POST',
      '${planPath(planId)}/lease',
      json: {'mode': leaseMode, if (force) 'force': true},
    ),
  );

  @override
  Future<HostedPlanLease> renewLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) async => _lease(
    await requestJson(
      session,
      'POST',
      '${planPath(planId)}/lease/renew',
      json: {'leaseId': leaseId},
    ),
  );

  @override
  Future<void> releaseLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) => requestNoContent(
    session,
    'POST',
    '${planPath(planId)}/lease/release',
    json: {'leaseId': leaseId},
  );

  @override
  Future<HostedPlanDownload> downloadPlan(
    AccountSession session,
    String planId,
  ) async {
    final response = await request(
      session,
      'GET',
      '${planPath(planId)}/download',
    );
    if (!response.ok) throw response.refused();
    final revision = response.header('x-domowik-revision');
    if (revision == null || revision.isEmpty) {
      throw const AccountException(
        'The server did not identify the downloaded revision',
      );
    }
    return HostedPlanDownload(bytes: response.body, revision: revision);
  }

  @override
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  ) async => _plan(
    await requestJson(
      session,
      'PUT',
      '${planPath(planId)}/content',
      bytes: bytes,
      headers: {'X-Domowik-Lease': leaseId, 'If-Match': '"$expectedRevision"'},
    ),
  );

  static HostedPlan _plan(Object? value) {
    if (value is! Map ||
        value['id'] is! String ||
        value['title'] is! String ||
        value['updatedAtEpoch'] is! int) {
      throw const AccountException('The server sent an unexpected plan');
    }
    final revision = value['revision'];
    return HostedPlan(
      id: value['id'] as String,
      title: value['title'] as String,
      revision: revision is String ? revision : '',
      updatedAt: DateTime.fromMillisecondsSinceEpoch(
        (value['updatedAtEpoch'] as int) * 1000,
      ),
    );
  }

  HostedPlanLease _lease(Object? value) {
    if (value is! Map ||
        value['leaseId'] is! String ||
        value['revision'] is! String) {
      throw const AccountException('The server sent an unexpected lease');
    }
    return HostedPlanLease(
      leaseId: value['leaseId'] as String,
      revision: value['revision'] as String,
      mode: value['mode'] is String ? value['mode'] as String : leaseMode,
      expiresInSeconds: value['expiresInSeconds'] is int
          ? value['expiresInSeconds'] as int
          : null,
    );
  }
}
