import 'dart:js_interop';

import '../account.dart';
import '../hosted_api.dart';
import '../property_planner.dart';

/// The page's server is set by the deployment, never by a preference.
const serverPreferenceAvailable = false;

/// The browser has no process environment to override the server.
String? serverUrlOverride() => null;

/// The account client when the deployment names a server, through
/// `<meta name="domowik-server-url">` or a `DOMOWIK_SERVER_URL` define; null
/// otherwise, which leaves the editor anonymous and local. The session is a
/// same-origin cookie, so the server must be the page's own origin.
/// [savedServerUrl] is a desktop preference and is ignored here.
AccountClient? createAccountClient({String? savedServerUrl}) {
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
class BrowserAccountClient extends HostedPlanProtocol
    implements AccountClient, PropertyPlannerClient {
  BrowserAccountClient(this.server, {void Function(String url)? navigate})
    : _navigate = navigate ?? _assign;

  @override
  final ServerEndpoint server;
  final void Function(String url) _navigate;

  @override
  String get leaseMode => 'browser';

  @override
  bool get signInLeavesEditor => true;

  /// The cookie travels by itself; writes add the session's CSRF token.
  @override
  Map<String, String> credentials(
    AccountSession session, {
    required bool mutation,
  }) => {if (mutation) 'X-Domowik-CSRF': session.token};

  /// A request before there is a session.
  Future<HostedResponse> _anonymous(
    String method,
    String path, {
    String? csrf,
  }) => transport(
    HostedRequest(
      method,
      server.api(path),
      headers: {if (csrf != null) 'X-Domowik-CSRF': csrf},
    ),
  );

  @override
  Future<AccountSession?> restore() async {
    final response = await _anonymous('GET', 'auth/session');
    if (response.status == 401) return null;
    return AccountSession(token: _field(response, 'csrfToken'));
  }

  @override
  Future<AccountSession?> signIn() async {
    final response = await _anonymous('POST', 'auth/sign-in', csrf: '1');
    // The page leaves (unless an unsaved-changes prompt keeps it); the
    // session is restored when the editor loads again.
    _navigate(_field(response, 'authorizationUrl'));
    return null;
  }

  @override
  Future<void> signOut(AccountSession session) =>
      requestNoContent(session, 'POST', 'auth/logout');

  @override
  Future<AccountProfile> profile(AccountSession session) async {
    final plans = requestJson(session, 'GET', 'plans')..ignore();
    final me = await requestJson(session, 'GET', 'auth/me');
    return AccountProfile.fromJson(me, await plans);
  }

  @override
  Future<PlannerPropertyOverview> propertyOverview(
    AccountSession session, {
    String? propertyHint,
  }) async => PlannerPropertyOverview.fromJson(
    await requestJson(
      session,
      'GET',
      'homehero/property',
      query: propertyHint == null ? null : {'propertyId': propertyHint},
    ),
  );

  @override
  Future<PlannerPlanContext> planPropertyContext(
    AccountSession session,
    String planId,
  ) async => PlannerPlanContext.fromJson(
    await requestJson(
      session,
      'GET',
      '${HostedPlanProtocol.planPath(planId)}/property',
    ),
  );

  @override
  Future<void> linkPlanProperty(
    AccountSession session,
    String planId,
    String propertyId,
  ) => requestNoContent(
    session,
    'PUT',
    '${HostedPlanProtocol.planPath(planId)}/property',
    json: {'propertyId': propertyId},
  );

  @override
  Future<void> unlinkPlanProperty(AccountSession session, String planId) =>
      requestNoContent(
        session,
        'DELETE',
        '${HostedPlanProtocol.planPath(planId)}/property',
      );

  @override
  Future<void> linkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
    String locationId,
  ) => requestNoContent(
    session,
    'PUT',
    '${_roomPath(planId, roomId)}/location',
    json: {'locationId': locationId},
  );

  @override
  Future<void> unlinkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
  ) => requestNoContent(
    session,
    'DELETE',
    '${_roomPath(planId, roomId)}/location',
  );

  @override
  Future<PlannerRoomInventory> roomInventory(
    AccountSession session,
    String planId,
    String roomId,
  ) async => PlannerRoomInventory.fromJson(
    await requestJson(session, 'GET', '${_roomPath(planId, roomId)}/inventory'),
  );

  @override
  Future<bool> openItemUri(Uri uri) async {
    final safe = validatedHomeHeroItemUri(uri.toString());
    if (safe == null) return false;
    _openWindow(safe.toString(), '_blank', 'noopener,noreferrer');
    return true;
  }

  static String _roomPath(String planId, String roomId) =>
      '${HostedPlanProtocol.planPath(planId)}/rooms/${Uri.encodeComponent(roomId)}';

  /// Same-origin `fetch`: the cookie is sent and redirects are refused.
  @override
  Future<HostedResponse> transport(HostedRequest request) async {
    try {
      final body = request.text?.toJS ?? request.bytes?.toJS;
      final response = await _fetch(
        request.url.toString(),
        _Init(
          method: request.method,
          credentials: 'same-origin',
          redirect: 'error',
          headers: request.headers.jsify()! as JSObject,
          body: body,
        ),
      ).toDart;
      final bytes = (await response.arrayBuffer().toDart).toDart.asUint8List();
      final headers = response.headers;
      return HostedResponse(
        response.status,
        bytes,
        (name) => headers.get(name),
      );
    } catch (_) {
      throw AccountException('Could not reach ${server.origin}');
    }
  }

  static String _field(HostedResponse response, String name) {
    final decoded = response.json;
    if (response.ok && decoded is Map && decoded[name] is String) {
      return decoded[name] as String;
    }
    throw response.refused();
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

@JS('window.open')
external JSAny? _openWindow(String url, String target, String features);

extension type _Location(JSObject _) implements JSObject {
  external void assign(String url);
}
