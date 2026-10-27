import 'dart:async';
import 'dart:convert';
import 'dart:io';

import '../account.dart';

/// The account client when `DOMOWIK_SERVER_URL` names a server, from the
/// environment or a `--dart-define`; null otherwise, and for an invalid URL,
/// which leaves the editor anonymous and local.
AccountClient? createAccountClient() {
  final configured =
      Platform.environment[serverUrlSetting] ??
      const String.fromEnvironment(serverUrlSetting);
  if (configured.trim().isEmpty) return null;
  try {
    return NativeAccountClient(ServerEndpoint(configured));
  } on FormatException {
    stderr.writeln(
      '$serverUrlSetting is not a valid server URL; signing in is off',
    );
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
/// its PKCE verifier are exchanged for an API token.
class NativeAccountClient implements AccountClient {
  NativeAccountClient(
    this.server, {
    this.openBrowser = openSystemBrowser,
    this.timeout = const Duration(minutes: 5),
  });

  @override
  final ServerEndpoint server;
  final BrowserOpener openBrowser;
  final Duration timeout;

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
      final issued = await _send(
        'POST',
        server.api('auth/desktop/token'),
        body: {
          'code': await code.timeout(
            timeout,
            onTimeout: () =>
                throw const AccountException('Signing in took too long'),
          ),
          'codeVerifier': verifier,
          'redirectUri': redirect,
        },
      );
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

  @override
  Future<void> signOut(AccountSession session) async {
    await _send(
      'DELETE',
      server.api('tokens/${session.tokenId}'),
      token: session.token,
      expectBody: false,
    );
  }

  @override
  Future<AccountProfile> profile(AccountSession session) async {
    final plans = _send('GET', server.api('plans'), token: session.token)
      ..ignore();
    final me = await _send('GET', server.api('auth/me'), token: session.token);
    return AccountProfile.fromJson(me, await plans);
  }

  /// The decoded JSON answer, or null when [expectBody] is false.
  Future<Object?> _send(
    String method,
    Uri url, {
    Map<String, String>? body,
    String? token,
    bool expectBody = true,
  }) async {
    final client = HttpClient()
      ..connectionTimeout = const Duration(seconds: 10);
    try {
      final request = await client.openUrl(method, url);
      request.followRedirects = false;
      if (token != null) {
        request.headers.set(HttpHeaders.authorizationHeader, 'Bearer $token');
      }
      if (body != null) {
        request.headers
          ..contentType = ContentType.json
          ..set('x-domowik-csrf', '1');
        request.write(jsonEncode(body));
      }
      final response = await request.close().timeout(
        const Duration(seconds: 30),
      );
      final text = await response
          .transform(utf8.decoder)
          .join()
          .timeout(const Duration(seconds: 30));
      if (response.statusCode < 200 || response.statusCode > 299) {
        throw AccountException(
          'The server refused (${response.statusCode}): ${_message(text)}',
        );
      }
      return expectBody ? jsonDecode(text) : null;
    } on FormatException {
      throw const AccountException('The server sent an unexpected answer');
    } on IOException {
      // Also a connection cut mid-answer (HttpException) or a TLS failure.
      throw AccountException('Could not reach ${server.origin}');
    } on TimeoutException {
      throw AccountException('${server.origin} did not answer');
    } finally {
      client.close(force: true);
    }
  }

  static String _message(String text) {
    try {
      final decoded = jsonDecode(text);
      if (decoded is Map && decoded['message'] is String) {
        return decoded['message'] as String;
      }
    } on FormatException {
      // Not JSON: fall through.
    }
    return 'no details';
  }
}
