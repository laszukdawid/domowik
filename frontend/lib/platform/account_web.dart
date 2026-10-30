import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';

import '../account.dart';

/// The account client when the deployment names a server, through
/// `<meta name="domowik-server-url">` or a `DOMOWIK_SERVER_URL` define; null
/// otherwise, which leaves the editor anonymous and local. The session is a
/// same-origin cookie, so the server must be the page's own origin.
AccountClient? createAccountClient() {
  final configured =
      _document
          .querySelector('meta[name="domowik-server-url"]')
          ?.getAttribute('content') ??
      const String.fromEnvironment(serverUrlSetting);
  if (configured.trim().isEmpty) return null;
  try {
    final server = ServerEndpoint(Uri.base.resolve(configured).origin);
    if (server.origin.origin != Uri.base.origin) return null;
    return BrowserAccountClient(server);
  } on FormatException {
    return null;
  }
}

/// Sign-in in the hosted editor: the server runs the sign-in and sets an
/// HttpOnly session cookie; the editor only learns the session's CSRF token.
class BrowserAccountClient implements AccountClient, HostedPlanClient {
  BrowserAccountClient(this.server, {void Function(String url)? navigate})
    : _navigate = navigate ?? _assign;

  @override
  final ServerEndpoint server;
  final void Function(String url) _navigate;

  @override
  Future<AccountSession?> restore() async {
    final (status, body) = await _send('GET', 'auth/session');
    if (status == 401) return null;
    final csrf = _field(status, body, 'csrfToken');
    return AccountSession(token: csrf);
  }

  @override
  Future<AccountSession?> signIn() async {
    final (status, body) = await _send('POST', 'auth/sign-in', csrf: '1');
    // The page leaves (unless an unsaved-changes prompt keeps it); the
    // session is restored when the editor loads again.
    _navigate(_field(status, body, 'authorizationUrl'));
    return null;
  }

  @override
  Future<void> signOut(AccountSession session) async {
    final (status, body) = await _send(
      'POST',
      'auth/logout',
      csrf: session.token,
    );
    if (status != 204) throw _refused(status, body);
  }

  @override
  Future<AccountProfile> profile(AccountSession session) async {
    final plans = _get('plans')..ignore();
    final me = await _get('auth/me');
    return AccountProfile.fromJson(me, await plans);
  }

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) async {
    final (status, body) = await _send('GET', 'plans');
    if (status < 200 || status > 299) throw _refused(status, body);
    final decoded = _json(body);
    if (decoded is! List) {
      throw const AccountException('The server sent an unexpected plan list');
    }
    return [for (final item in decoded) _plan(item)];
  }

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) async {
    final (status, body) = await _send(
      'POST',
      'plans',
      csrf: session.token,
      jsonBody: {'title': title},
    );
    if (status < 200 || status > 299) throw _refused(status, body);
    return _plan(_json(body));
  }

  @override
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  ) async {
    final (status, body) = await _send(
      'POST',
      'plans/import',
      query: {'title': title},
      csrf: session.token,
      binaryBody: bytes,
    );
    if (status < 200 || status > 299) throw _refused(status, body);
    return _plan(_json(body));
  }

  @override
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  }) async {
    final (status, body) = await _send(
      'POST',
      '${_planPath(planId)}/lease',
      csrf: session.token,
      jsonBody: {'mode': 'browser', if (force) 'force': true},
    );
    if (status < 200 || status > 299) throw _refused(status, body);
    return _lease(_json(body));
  }

  @override
  Future<HostedPlanLease> renewLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) async {
    final (status, body) = await _send(
      'POST',
      '${_planPath(planId)}/lease/renew',
      csrf: session.token,
      jsonBody: {'leaseId': leaseId},
    );
    if (status < 200 || status > 299) throw _refused(status, body);
    return _lease(_json(body));
  }

  @override
  Future<void> releaseLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) async {
    final (status, body) = await _send(
      'POST',
      '${_planPath(planId)}/lease/release',
      csrf: session.token,
      jsonBody: {'leaseId': leaseId},
    );
    if (status != 204) throw _refused(status, body);
  }

  @override
  Future<HostedPlanDownload> downloadPlan(
    AccountSession session,
    String planId,
  ) async {
    final response = await _response('GET', '${_planPath(planId)}/download');
    if (response.status < 200 || response.status > 299) {
      throw _refused(response.status, (await response.text().toDart).toDart);
    }
    final revision = response.headers.get('x-domowik-revision');
    if (revision == null || revision.isEmpty) {
      throw const AccountException(
        'The server did not identify the downloaded revision',
      );
    }
    final bytes = (await response.arrayBuffer().toDart).toDart.asUint8List();
    return HostedPlanDownload(bytes: bytes, revision: revision);
  }

  @override
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  ) async {
    final (status, body) = await _send(
      'PUT',
      '${_planPath(planId)}/content',
      csrf: session.token,
      binaryBody: bytes,
      extraHeaders: {
        'X-Domowik-Lease': leaseId,
        'If-Match': '"$expectedRevision"',
      },
    );
    if (status < 200 || status > 299) throw _refused(status, body);
    return _plan(_json(body));
  }

  /// The decoded JSON answer of a read, which needs no CSRF token.
  Future<Object?> _get(String path) async {
    final (status, body) = await _send('GET', path);
    if (status < 200 || status > 299) throw _refused(status, body);
    return _json(body);
  }

  Future<(int, String)> _send(
    String method,
    String path, {
    String? csrf,
    Map<String, String>? query,
    Map<String, Object?>? jsonBody,
    Uint8List? binaryBody,
    Map<String, String> extraHeaders = const {},
  }) async {
    try {
      final response = await _response(
        method,
        path,
        csrf: csrf,
        query: query,
        jsonBody: jsonBody,
        binaryBody: binaryBody,
        extraHeaders: extraHeaders,
      );
      return (response.status, (await response.text().toDart).toDart);
    } catch (_) {
      throw AccountException('Could not reach ${server.origin}');
    }
  }

  Future<_Response> _response(
    String method,
    String path, {
    String? csrf,
    Map<String, String>? query,
    Map<String, Object?>? jsonBody,
    Uint8List? binaryBody,
    Map<String, String> extraHeaders = const {},
  }) async {
    try {
      final headers = <String, String>{
        if (csrf != null) 'X-Domowik-CSRF': csrf,
        ...extraHeaders,
        if (jsonBody != null) 'Content-Type': 'application/json',
        if (binaryBody != null) 'Content-Type': 'application/octet-stream',
      };
      final body = jsonBody != null
          ? jsonEncode(jsonBody).toJS
          : binaryBody?.toJS;
      return await _fetch(
        server.api(path, query).toString(),
        _Init(
          method: method,
          credentials: 'same-origin',
          redirect: 'error',
          headers: headers.jsify()! as JSObject,
          body: body,
        ),
      ).toDart;
    } catch (_) {
      throw AccountException('Could not reach ${server.origin}');
    }
  }

  static String _planPath(String id) => 'plans/${Uri.encodeComponent(id)}';

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

  static HostedPlanLease _lease(Object? value) {
    if (value is! Map ||
        value['leaseId'] is! String ||
        value['revision'] is! String) {
      throw const AccountException('The server sent an unexpected lease');
    }
    return HostedPlanLease(
      leaseId: value['leaseId'] as String,
      revision: value['revision'] as String,
      mode: value['mode'] is String ? value['mode'] as String : 'browser',
      expiresInSeconds: value['expiresInSeconds'] is int
          ? value['expiresInSeconds'] as int
          : null,
    );
  }

  static Object? _json(String body) {
    try {
      return jsonDecode(body);
    } on FormatException {
      return null;
    }
  }

  static String _field(int status, String body, String name) {
    final decoded = _json(body);
    if (status >= 200 &&
        status < 300 &&
        decoded is Map &&
        decoded[name] is String) {
      return decoded[name] as String;
    }
    throw _refused(status, body);
  }

  static AccountException _refused(int status, String body) {
    final decoded = _json(body);
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

void _assign(String url) => _location.assign(url);

@JS('fetch')
external JSPromise<_Response> _fetch(String url, _Init init);

extension type _Init._(JSObject _) implements JSObject {
  external factory _Init({
    String method,
    String credentials,
    String redirect,
    JSObject headers,
    JSAny? body,
  });
}

extension type _Response(JSObject _) implements JSObject {
  external int get status;
  external JSPromise<JSString> text();
  external _Headers get headers;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

extension type _Headers(JSObject _) implements JSObject {
  external String? get(String name);
}

@JS('document')
external _Document get _document;

extension type _Document(JSObject _) implements JSObject {
  external _Element? querySelector(String selector);
}

extension type _Element(JSObject _) implements JSObject {
  external String? getAttribute(String name);
}

@JS('location')
external _Location get _location;

extension type _Location(JSObject _) implements JSObject {
  external void assign(String url);
}
