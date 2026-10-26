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

/// A one-off PKCE verifier and its S256 challenge (RFC 7636).
class Pkce {
  Pkce._(this.verifier)
    : challenge = base64Url
          .encode(sha256(ascii.encode(verifier)))
          .replaceAll('=', '');

  factory Pkce.create([Random? random]) =>
      Pkce._(randomToken(32, random ?? Random.secure()));

  final String verifier;
  final String challenge;
}

/// `bytes` random bytes as unpadded base64url.
String randomToken(int bytes, Random random) => base64Url
    .encode(List<int>.generate(bytes, (_) => random.nextInt(256)))
    .replaceAll('=', '');

/// A signed-in account, for this run of the app only: the API token is kept
/// in memory and never written to disk.
class AccountSession {
  const AccountSession({
    required this.username,
    required this.token,
    required this.tokenId,
  });

  final String username;
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
  Future<AccountSession> signIn();
  Future<void> signOut(AccountSession session);
}
