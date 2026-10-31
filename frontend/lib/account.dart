import 'dart:convert';
import 'dart:math';
import 'dart:typed_data';

import 'sha256.dart';

/// Signing in to a Domowik server is optional. Without a configured server the
/// editor stays an anonymous, local-only app and makes no request.
///
/// The server alone handles accounts: the editor knows only its URL and the
/// routes it serves (`/api/v1/auth/desktop` and `/api/v1/auth/desktop/token`,
/// and `/api/v1/auth/me` and `/api/v1/plans` for the account menu), never how
/// the server signs people in.
///
/// On the desktop this environment variable or build define overrides the
/// Server preference; an empty value turns accounts off.
const serverUrlSetting = 'DOMOWIK_SERVER_URL';

/// The desktop app's server when the preference was never set.
const defaultServerUrl = 'https://planner.homehero.pro';

/// The desktop app's server URL: an [override] wins, then the [saved]
/// preference, then [defaultServerUrl]. Empty means no server.
String effectiveServerUrl({String? override, String? saved}) =>
    (override ?? saved ?? defaultServerUrl).trim();

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

  /// `scheme://host[:port]`, so equivalent spellings compare equal.
  String get canonical => origin.origin;

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
/// this run of the app only and never written to disk. Browser: the session's CSRF token (the session
/// itself is an HttpOnly cookie) and no id.
class AccountSession {
  const AccountSession({required this.token, this.tokenId = ''});

  final String token;
  final String tokenId;
}

/// A plan kept on the server.
class HostedPlan {
  const HostedPlan({
    required this.title,
    required this.updatedAt,
    this.id = '',
    this.revision = '',
  });

  final String id;
  final String title;

  /// The server's opaque decimal revision. Keep it as a string so it remains
  /// exact even if the service later moves beyond JavaScript's safe integers.
  final String revision;
  final DateTime updatedAt;
}

/// A hosted writer lease. [revision] is the checkout revision and is
/// deliberately kept as a string for exact `If-Match` round trips.
class HostedPlanLease {
  const HostedPlanLease({
    required this.leaseId,
    required this.revision,
    this.mode = 'browser',
    this.expiresInSeconds,
  });

  final String leaseId;
  final String revision;
  final String mode;
  final int? expiresInSeconds;
}

class HostedPlanDownload {
  const HostedPlanDownload({required this.bytes, required this.revision});

  final Uint8List bytes;
  final String revision;
}

/// Optional plan operations. Kept separate from [AccountClient] so
/// account-only integrations remain compatible.
abstract interface class HostedPlanClient {
  Future<List<HostedPlan>> listPlans(AccountSession session);
  Future<HostedPlan> createPlan(AccountSession session, String title);
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  );
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  });
  Future<HostedPlanLease> renewLease(
    AccountSession session,
    String planId,
    String leaseId,
  );
  Future<void> releaseLease(
    AccountSession session,
    String planId,
    String leaseId,
  );
  Future<HostedPlanDownload> downloadPlan(
    AccountSession session,
    String planId,
  );
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  );
}

/// Who is signed in and what their account holds on the server.
class AccountProfile {
  const AccountProfile({
    required this.email,
    required this.plans,
    this.planLimit,
  });

  /// Reads the answers of `GET auth/me` and `GET plans`. [planLimit] is null
  /// when the server does not report one.
  factory AccountProfile.fromJson(Object? me, Object? plans) {
    if (me is! Map || me['username'] is! String || plans is! List) {
      throw const AccountException('The server sent an unexpected answer');
    }
    final limit = me['planLimit'];
    return AccountProfile(
      email: me['username'] as String,
      planLimit: limit is int ? limit : null,
      plans: [
        for (final plan in plans)
          if (plan is Map &&
              plan['title'] is String &&
              plan['updatedAtEpoch'] is int)
            HostedPlan(
              id: plan['id'] is String ? plan['id'] as String : '',
              title: plan['title'] as String,
              revision: plan['revision'] is String
                  ? plan['revision'] as String
                  : '',
              updatedAt: DateTime.fromMillisecondsSinceEpoch(
                (plan['updatedAtEpoch'] as int) * 1000,
              ),
            ),
      ],
    );
  }

  final String email;
  final List<HostedPlan> plans;
  final int? planLimit;
}

class AccountException implements Exception {
  const AccountException(this.message, {this.statusCode, this.code});
  final String message;
  final int? statusCode;
  final String? code;
  @override
  String toString() => message;
}

/// Signs in and out of a Domowik server.
abstract interface class AccountClient {
  ServerEndpoint get server;

  /// Whether signing in leaves the editor (a browser page navigates to the
  /// server), so unsaved work must be settled first.
  bool get signInLeavesEditor;

  /// The session already in place when the editor starts, if any.
  Future<AccountSession?> restore();

  /// Signs in. In a browser the page leaves for the server's sign-in and
  /// this returns null; the session is restored when the editor loads again.
  Future<AccountSession?> signIn();
  Future<void> signOut(AccountSession session);

  /// The signed-in account and its plans on the server.
  Future<AccountProfile> profile(AccountSession session);
}
