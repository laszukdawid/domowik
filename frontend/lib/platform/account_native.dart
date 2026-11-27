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

/// Stored desktop API token metadata for one canonical server origin.
class StoredDesktopSession {
  const StoredDesktopSession({
    required this.token,
    required this.tokenId,
    this.expiresAtEpoch,
  });

  final String token;
  final String tokenId;
  final int? expiresAtEpoch;

  bool isExpiredAt(DateTime now) =>
      expiresAtEpoch != null &&
      now.millisecondsSinceEpoch ~/ 1000 >= expiresAtEpoch!;
}

/// Persistent storage for desktop API tokens, keyed by canonical server origin.
abstract interface class DesktopTokenStore {
  Future<StoredDesktopSession?> read(String serverOrigin);
  Future<void> write(String serverOrigin, StoredDesktopSession session);
  Future<void> delete(
    String serverOrigin, {
    String? matchingToken,
    String? matchingTokenId,
  });
}

/// In-memory [DesktopTokenStore] for isolated unit tests.
class MemoryDesktopTokenStore implements DesktopTokenStore {
  final Map<String, StoredDesktopSession> _sessions = {};

  @override
  Future<StoredDesktopSession?> read(String serverOrigin) async =>
      _sessions[serverOrigin];

  @override
  Future<void> write(String serverOrigin, StoredDesktopSession session) async {
    _sessions[serverOrigin] = session;
  }

  @override
  Future<void> delete(
    String serverOrigin, {
    String? matchingToken,
    String? matchingTokenId,
  }) async {
    final existing = _sessions[serverOrigin];
    if (existing == null) return;
    if (!_sessionMatches(
      existing,
      matchingToken: matchingToken,
      matchingTokenId: matchingTokenId,
    )) {
      return;
    }
    _sessions.remove(serverOrigin);
  }
}

bool _sessionMatches(
  StoredDesktopSession existing, {
  String? matchingToken,
  String? matchingTokenId,
}) {
  if (matchingToken == null && matchingTokenId == null) return true;
  if (matchingToken != null && existing.token == matchingToken) return true;
  if (matchingTokenId != null &&
      matchingTokenId.isNotEmpty &&
      existing.tokenId == matchingTokenId) {
    return true;
  }
  return false;
}

/// Owner-only file store for desktop API tokens. On POSIX hosts the parent
/// directory (`0700`) and temporary sibling (`0600`) are restricted before any
/// secret bytes are written, then atomically renamed into place.
class FileDesktopTokenStore implements DesktopTokenStore {
  FileDesktopTokenStore(this.file);

  final File file;

  /// Bounds read size so a damaged file cannot exhaust memory.
  static const int maxBytes = 64 * 1024;

  /// Bounds remembered server origins so switching servers cannot grow without
  /// bound.
  static const int maxServers = 16;

  Future<void>? _pending;

  Future<T> _serialized<T>(Future<T> Function() action) {
    final previous = _pending;
    final run = previous == null ? action() : previous.then((_) => action());
    late final Future<void> settled;
    void clear() {
      if (identical(_pending, settled)) _pending = null;
    }

    settled = run.then<void>((_) => clear(), onError: (Object _) => clear());
    _pending = settled;
    return run;
  }

  @override
  Future<StoredDesktopSession?> read(String serverOrigin) =>
      _serialized(() async {
        final all = await _readAll();
        return all[serverOrigin];
      });

  @override
  Future<void> write(String serverOrigin, StoredDesktopSession session) =>
      _serialized(() async {
        final next = await _readAll();
        next.remove(serverOrigin);
        next[serverOrigin] = session;
        while (next.length > maxServers) {
          next.remove(next.keys.first);
        }
        await _writeAll(next);
      });

  @override
  Future<void> delete(
    String serverOrigin, {
    String? matchingToken,
    String? matchingTokenId,
  }) => _serialized(() async {
    final next = await _readAll();
    final existing = next[serverOrigin];
    if (existing == null) return;
    if (!_sessionMatches(
      existing,
      matchingToken: matchingToken,
      matchingTokenId: matchingTokenId,
    )) {
      return;
    }
    next.remove(serverOrigin);
    if (next.isEmpty) {
      if (await file.exists()) await file.delete();
      return;
    }
    await _writeAll(next);
  });

  Future<Map<String, StoredDesktopSession>> _readAll() async {
    if (!await file.exists()) return {};
    if (!Platform.isWindows) {
      final stat = await file.stat();
      if ((stat.mode & 0x03f) != 0) {
        await _restrictPosixMode(file.path, '600');
      }
    }
    if (await file.length() > maxBytes) return {};
    final RandomAccessFile handle = await file.open();
    final List<int> bytes;
    try {
      bytes = await handle.read(maxBytes + 1);
    } finally {
      await handle.close();
    }
    if (bytes.length > maxBytes) return {};
    try {
      final decoded = jsonDecode(utf8.decode(bytes));
      if (decoded is! Map || decoded['version'] != 1) return {};
      final rawSessions = decoded['sessions'];
      if (rawSessions is! Map) return {};
      final parsed = <String, StoredDesktopSession>{};
      for (final entry in rawSessions.entries) {
        final origin = entry.key;
        final value = entry.value;
        if (origin is! String || origin.isEmpty || value is! Map) continue;
        final token = value['token'];
        final tokenId = value['tokenId'];
        final expires = value['expiresAtEpoch'];
        if (token is! String ||
            token.isEmpty ||
            tokenId is! String ||
            tokenId.isEmpty ||
            (expires != null && expires is! int)) {
          continue;
        }
        parsed[origin] = StoredDesktopSession(
          token: token,
          tokenId: tokenId,
          expiresAtEpoch: expires as int?,
        );
      }
      return parsed;
    } on FormatException {
      return {};
    }
  }

  Future<void> _writeAll(Map<String, StoredDesktopSession> sessions) async {
    await file.parent.create(recursive: true);
    if (!Platform.isWindows) {
      await _restrictPosixMode(file.parent.path, '700');
    }
    final temporary = File(
      '${file.path}.$pid.${DateTime.now().microsecondsSinceEpoch}.tmp',
    );
    try {
      await temporary.writeAsBytes(const [], flush: true);
      if (Platform.isWindows) {
        await _restrictWindowsAcl(temporary.path);
      } else {
        await _restrictPosixMode(temporary.path, '600');
      }
      final payload = jsonEncode({
        'version': 1,
        'sessions': {
          for (final entry in sessions.entries)
            entry.key: {
              'token': entry.value.token,
              'tokenId': entry.value.tokenId,
              if (entry.value.expiresAtEpoch != null)
                'expiresAtEpoch': entry.value.expiresAtEpoch,
            },
        },
      });
      await temporary.writeAsString('$payload\n', flush: true);
      await temporary.rename(file.path);
    } finally {
      if (await temporary.exists()) await temporary.delete();
    }
  }

  static Future<void> _restrictPosixMode(String path, String mode) async {
    final result = await Process.run('chmod', [mode, path]);
    if (result.exitCode != 0) {
      throw FileSystemException(
        'Could not restrict permissions on credential path',
        path,
      );
    }
  }

  static Future<void> _restrictWindowsAcl(String path) async {
    final user = Platform.environment['USERNAME'];
    if (user == null || user.isEmpty) return;
    try {
      await Process.run('icacls', [
        path,
        '/inheritance:r',
        '/grant:r',
        '$user:(R,W)',
      ]);
    } on ProcessException {
      // Best-effort ACL tightening when icacls is unavailable.
    }
  }
}

/// Resolves the per-user private token file path for [operatingSystem] and
/// [environment], or null when no user data directory is available.
String? desktopTokenFilePath({
  Map<String, String>? environment,
  String? operatingSystem,
}) {
  final env = environment ?? Platform.environment;
  final os = operatingSystem ?? Platform.operatingSystem;
  if (os != 'macos' &&
      os != 'windows' &&
      env['XDG_CONFIG_HOME']?.startsWith('/') == true) {
    return '${env['XDG_CONFIG_HOME']}/domowik/auth/tokens.json';
  }
  if (os == 'windows') {
    final base =
        env['LOCALAPPDATA'] ??
        env['APPDATA'] ??
        (env['USERPROFILE'] != null
            ? '${env['USERPROFILE']}/AppData/Local'
            : null);
    if (base == null || base.isEmpty) return null;
    return '$base/domowik/auth/tokens.json';
  }
  final home = env['HOME'] ?? env['USERPROFILE'];
  if (home == null || home.isEmpty) return null;
  final base = os == 'macos'
      ? '$home/Library/Application Support'
      : '$home/.config';
  return '$base/domowik/auth/tokens.json';
}

/// The platform's default desktop token store, or null when the environment
/// exposes no user directory. Constructing the store performs no I/O.
DesktopTokenStore? standardDesktopTokenStore({
  Map<String, String>? environment,
  String? operatingSystem,
}) {
  final path = desktopTokenFilePath(
    environment: environment,
    operatingSystem: operatingSystem,
  );
  return path == null ? null : FileDesktopTokenStore(File(path));
}

/// The account client for the effective server (see [effectiveServerUrl]);
/// null when it is empty or invalid, which leaves the editor anonymous and
/// local. Creating a client makes no request.
AccountClient? createAccountClient({
  String? savedServerUrl,
  DesktopTokenStore? tokenStore,
}) {
  final configured = effectiveServerUrl(
    override: serverUrlOverride(),
    saved: savedServerUrl,
  );
  if (configured.isEmpty) return null;
  try {
    return NativeAccountClient(
      ServerEndpoint(configured),
      tokenStore: tokenStore ?? standardDesktopTokenStore(),
    );
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
    this.tokenStore,
    DateTime Function()? clock,
  }) : _clock = clock ?? DateTime.now;

  /// The largest answer read: the service's expanded plan limit.
  static const maxResponseBytes = 64 * 1024 * 1024;

  @override
  final ServerEndpoint server;
  final BrowserOpener openBrowser;
  final Duration timeout;

  /// Bounds the wait for an answer to start and each pause while it arrives.
  final Duration responseTimeout;

  /// Optional persistent token store across restarts.
  final DesktopTokenStore? tokenStore;
  final DateTime Function() _clock;

  @override
  String get leaseMode => 'api';

  @override
  bool get signInLeavesEditor => false;

  @override
  Map<String, String> credentials(
    AccountSession session, {
    required bool mutation,
  }) => {'Authorization': 'Bearer ${session.token}'};

  /// Restores a previously saved desktop API token for [server]. Expired or
  /// server-rejected (401/403) tokens are cleared locally; transient network
  /// errors preserve the stored token so offline launches do not force browser
  /// re-authentication.
  @override
  Future<AccountSession?> restore() async {
    final store = tokenStore;
    if (store == null) return null;
    StoredDesktopSession? stored;
    try {
      stored = await store.read(server.canonical);
    } on IOException {
      return null;
    }
    if (stored == null) return null;
    if (stored.token.isEmpty ||
        stored.tokenId.isEmpty ||
        stored.isExpiredAt(_clock())) {
      await _deleteStoredSession(
        AccountSession(token: stored.token, tokenId: stored.tokenId),
      );
      return null;
    }
    final session = AccountSession(
      token: stored.token,
      tokenId: stored.tokenId,
    );
    try {
      final response = await request(session, 'GET', 'auth/me');
      if (response.status == HttpStatus.unauthorized ||
          response.status == HttpStatus.forbidden) {
        await _deleteStoredSession(session);
        return null;
      }
    } on AccountException {
      // Offline or unreachable: retain the persisted token until reachable.
    }
    return session;
  }

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
      final token = issued['token'] as String;
      final tokenId = issued['id'] as String;
      final expiresIn = issued['expiresInSeconds'];
      final expiresAtEpoch = expiresIn is int && expiresIn > 0
          ? (_clock().millisecondsSinceEpoch ~/ 1000) + expiresIn
          : null;
      await _writeStoredSession(
        StoredDesktopSession(
          token: token,
          tokenId: tokenId,
          expiresAtEpoch: expiresAtEpoch,
        ),
      );
      return AccountSession(token: token, tokenId: tokenId);
    } finally {
      await listener.close(force: true);
    }
  }

  Future<void> _writeStoredSession(StoredDesktopSession session) async {
    final store = tokenStore;
    if (store == null) return;
    try {
      await store.write(server.canonical, session);
    } on IOException {
      // Unwritable local storage must not abort an active in-memory sign-in.
    }
  }

  Future<void> _deleteStoredSession(AccountSession session) async {
    final store = tokenStore;
    if (store == null) return;
    try {
      await store.delete(
        server.canonical,
        matchingToken: session.token,
        matchingTokenId: session.tokenId,
      );
    } on IOException {
      // Best-effort local cleanup when storage is inaccessible.
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

  /// Revokes the token on the server and always removes any matching persisted
  /// local token, even if the network or server fails. Tokens the server no
  /// longer knows (401 or 404) are already inactive.
  @override
  Future<void> signOut(AccountSession session) async {
    try {
      if (session.tokenId.isNotEmpty) {
        final response = await request(
          session,
          'DELETE',
          'tokens/${Uri.encodeComponent(session.tokenId)}',
        );
        if (!response.ok &&
            response.status != HttpStatus.unauthorized &&
            response.status != HttpStatus.notFound) {
          throw response.refused();
        }
      }
    } finally {
      await _deleteStoredSession(session);
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
