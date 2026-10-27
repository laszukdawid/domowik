import 'dart:async';
import 'dart:convert';
import 'dart:js_interop';

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
class BrowserAccountClient implements AccountClient {
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
  Future<AccountSession> signIn() async {
    final (status, body) = await _send('POST', 'auth/sign-in', csrf: '1');
    _navigate(_field(status, body, 'authorizationUrl'));
    // The page is leaving; the session is restored when it loads again.
    return Completer<AccountSession>().future;
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

  Future<(int, String)> _send(
    String method,
    String path, {
    String? csrf,
  }) async {
    try {
      final response = await _fetch(
        server.api(path).toString(),
        _Init(
          method: method,
          credentials: 'same-origin',
          redirect: 'error',
          headers:
              {if (csrf != null) 'X-Domowik-CSRF': csrf}.jsify()! as JSObject,
        ),
      ).toDart;
      return (response.status, (await response.text().toDart).toDart);
    } catch (_) {
      throw AccountException('Could not reach ${server.origin}');
    }
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
    return AccountException('The server refused ($status): $message');
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
  });
}

extension type _Response(JSObject _) implements JSObject {
  external int get status;
  external JSPromise<JSString> text();
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
