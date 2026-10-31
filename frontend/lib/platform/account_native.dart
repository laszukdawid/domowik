import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../account.dart';
import '../hosted_api.dart';

/// The desktop app offers the Server preference.
const serverPreferenceAvailable = true;

/// The `DOMOWIK_SERVER_URL` override from the environment, then the build
/// define; null when neither is set. An empty value turns accounts off.
String? serverUrlOverride() =>
    Platform.environment[serverUrlSetting] ??
    (const bool.hasEnvironment(serverUrlSetting)
        ? const String.fromEnvironment(serverUrlSetting)
        : null);

/// The account client for the effective server (see [effectiveServerUrl]);
/// null when it is empty or invalid, which leaves the editor anonymous and
/// local. Creating a client makes no request.
AccountClient? createAccountClient({String? savedServerUrl}) {
  final configured = effectiveServerUrl(
    override: serverUrlOverride(),
    saved: savedServerUrl,
  );
  if (configured.isEmpty) return null;
  try {
    return NativeAccountClient(ServerEndpoint(configured));
  } on FormatException {
    stderr.writeln('$configured is not a valid server URL; signing in is off');
    return null;
  }
}

/// Opens `url` in the system browser.
typedef BrowserOpener = Future<void> Function(Uri url);

/// Launches the browser without waiting for it: on some systems the launcher
/// stays in the foreground for as long as the browser runs.
Future<void> openSystemBrowser(Uri url) async {
  final (command, arguments) = Platform.isMacOS
      ? ('open', [url.toString()])
      : Platform.isWindows
      ? ('rundll32', ['url.dll,FileProtocolHandler', url.toString()])
      : ('xdg-open', [url.toString()]);
  try {
    await Process.start(command, arguments, mode: ProcessStartMode.detached);
  } on ProcessException {
    throw const AccountException('Could not open the web browser');
  }
}

/// Sign-in for a native app (RFC 8252): the system browser signs in at the
/// server, which returns a one-time code to a loopback listener; the code and
/// its PKCE verifier are exchanged for an API token. Plan requests send that
/// token as `Authorization: Bearer` and take `api` writer leases.
class NativeAccountClient extends HostedPlanProtocol implements AccountClient {
  NativeAccountClient(
    this.server, {
    this.openBrowser = openSystemBrowser,
    this.timeout = const Duration(minutes: 5),
    this.responseTimeout = const Duration(seconds: 30),
  });

  /// The largest answer read: the service's expanded plan limit.
  static const maxResponseBytes = 64 * 1024 * 1024;

  @override
  final ServerEndpoint server;
  final BrowserOpener openBrowser;
  final Duration timeout;

  /// Bounds the wait for an answer to start and each pause while it arrives.
  final Duration responseTimeout;

  @override
  String get leaseMode => 'api';

  @override
  bool get signInLeavesEditor => false;

  @override
  Map<String, String> credentials(
    AccountSession session, {
    required bool mutation,
  }) => {'Authorization': 'Bearer ${session.token}'};

  /// Nothing outlives the app: every run starts signed out.
  @override
  Future<AccountSession?> restore() async => null;

  @override
  Future<AccountSession?> signIn() async {
    final verifier = nonce(32);
    final state = nonce(16);
    final listener = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    final redirect = 'http://127.0.0.1:${listener.port}/callback';
    try {
      final code = _awaitCode(listener, state)..ignore();
      await openBrowser(
        server.api('auth/desktop', {
          'redirect_uri': redirect,
          'state': state,
          'code_challenge': s256(verifier),
          'code_challenge_method': 'S256',
        }),
      );
      final body = jsonEncode({
        'code': await code.timeout(
          timeout,
          onTimeout: () =>
              throw const AccountException('Signing in took too long'),
        ),
        'codeVerifier': verifier,
        'redirectUri': redirect,
      });
      // The exchange has no session yet; the service still wants its
      // pre-authentication header.
      final response = await transport(
        HostedRequest(
          'POST',
          server.api('auth/desktop/token'),
          headers: {'Content-Type': 'application/json', 'X-Domowik-CSRF': '1'},
          text: body,
        ),
      );
      if (!response.ok) throw response.refused();
      final issued = response.json;
      if (issued is! Map ||
          issued['token'] is! String ||
          issued['id'] is! String) {
        throw const AccountException('The server sent an unexpected answer');
      }
      return AccountSession(
        token: issued['token'] as String,
        tokenId: issued['id'] as String,
      );
    } finally {
      await listener.close(force: true);
    }
  }

  /// The code from the first callback carrying this sign-in's `state`.
  Future<String> _awaitCode(HttpServer listener, String state) async {
    await for (final request in listener) {
      final query = request.uri.queryParameters;
      final ours = request.uri.path == '/callback' && query['state'] == state;
      final code = query['code'];
      request.response
        ..statusCode = ours ? HttpStatus.ok : HttpStatus.notFound
        ..write(ours ? 'Return to Domowik.' : 'Not found.');
      await request.response.close();
      if (!ours) continue;
      if (code == null || code.isEmpty) {
        throw const AccountException('Signing in was not completed');
      }
      return code;
    }
    throw const AccountException('Signing in was interrupted');
  }

  /// Revokes the token. A token the server no longer knows (401) is already
  /// signed out.
  @override
  Future<void> signOut(AccountSession session) async {
    final response = await request(
      session,
      'DELETE',
      'tokens/${Uri.encodeComponent(session.tokenId)}',
    );
    if (!response.ok && response.status != HttpStatus.unauthorized) {
      throw response.refused();
    }
  }

  @override
  Future<AccountProfile> profile(AccountSession session) async {
    final plans = requestJson(session, 'GET', 'plans')..ignore();
    final me = await requestJson(session, 'GET', 'auth/me');
    return AccountProfile.fromJson(me, await plans);
  }

  /// One request without redirects; the answer is bounded in time and size.
  @override
  Future<HostedResponse> transport(HostedRequest request) async {
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 10);
    try {
      final outgoing = await client.openUrl(request.method, request.url);
      outgoing.followRedirects = false;
      request.headers.forEach(outgoing.headers.set);
      final body =
          request.bytes ??
          (request.text == null ? null : utf8.encode(request.text!));
      if (body != null) {
        outgoing.contentLength = body.length;
        outgoing.add(body);
      }
      final response = await outgoing.close().timeout(responseTimeout);
      final received = BytesBuilder(copy: false);
      await for (final chunk in response.timeout(responseTimeout)) {
        if (received.length + chunk.length > maxResponseBytes) {
          throw const AccountException('The server sent too large an answer');
        }
        received.add(chunk);
      }
      return HostedResponse(response.statusCode, received.takeBytes(), (name) {
        try {
          return response.headers.value(name);
        } on HttpException {
          return null; // Repeated: not a usable single value.
        }
      });
    } on IOException {
      // Also a connection cut mid-answer (HttpException) or a TLS failure.
      throw AccountException('Could not reach ${server.origin}');
    } on TimeoutException {
      throw AccountException('${server.origin} did not answer');
    } finally {
      client.close(force: true);
    }
  }
}
