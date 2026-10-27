import 'dart:convert';
import 'dart:math';

import 'sha256.dart';

/// Signing in to a Domowik server is optional. Without a configured server the
/// editor stays an anonymous, local-only app and makes no request.
///
/// The server alone handles accounts: the editor knows only its URL and the
/// routes it serves (`/api/v1/auth/desktop` and `/api/v1/auth/desktop/token`),
/// never how the server signs people in.
const serverUrlSetting = 'DOMOWIK_SERVER_URL';

/// The configured server: an HTTPS origin, or plain HTTP on loopback for local
/// development. No path, query, fragment or credentials.
class ServerEndpoint {
  ServerEndpoint(String url) : origin = Uri.parse(url.trim()) {
    final loopback = const [
      'localhost',
      '127.0.0.1',
      '::1',
    ].contains(origin.host);
    if (!(origin.scheme == 'https' || (origin.scheme == 'http' && loopback)) ||
        origin.host.isEmpty ||
        origin.userInfo.isNotEmpty ||
        !(origin.path.isEmpty || origin.path == '/') ||
        origin.hasQuery ||
        origin.hasFragment) {
      throw const FormatException('Invalid Domowik server URL');
    }
  }

  final Uri origin;

  Uri api(String path, [Map<String, String>? query]) =>
      origin.replace(path: '/api/v1/$path', queryParameters: query);
}

/// A secure random value as unpadded base64url: 32 bytes for a PKCE verifier
/// (RFC 7636), 16 for a `state`.
String nonce(int bytes) {
  final random = Random.secure();
  return base64Url
      .encode(List<int>.generate(bytes, (_) => random.nextInt(256)))
      .replaceAll('=', '');
}

/// The S256 challenge of a PKCE verifier.
String s256(String verifier) =>
    base64Url.encode(sha256(ascii.encode(verifier))).replaceAll('=', '');

/// A signed-in account. Desktop: an API token and its id, kept in memory for
/// this run of the app only. Browser: the session's CSRF token (the session
/// itself is an HttpOnly cookie) and no id.
class AccountSession {
  const AccountSession({required this.token, this.tokenId = ''});

  final String token;
  final String tokenId;
}

class AccountException implements Exception {
  const AccountException(this.message);
  final String message;
  @override
  String toString() => message;
}

/// Signs in and out of a Domowik server.
abstract interface class AccountClient {
  ServerEndpoint get server;

  /// The session already in place when the editor starts, if any.
  Future<AccountSession?> restore();

  /// Signs in. In a browser the page leaves for the server's sign-in and this
  /// never completes; the session is restored when the editor loads again.
  Future<AccountSession> signIn();
  Future<void> signOut(AccountSession session);
}
