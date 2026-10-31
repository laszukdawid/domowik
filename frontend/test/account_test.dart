import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/platform/account_native.dart';
import 'package:domowik_editor/sha256.dart';
import 'package:flutter_test/flutter_test.dart';

/// A stand-in for the Domowik server's desktop sign-in routes.
class FakeServer {
  late final HttpServer _server;
  String? challenge;
  bool refuseToken = false;
  final revoked = <String>[];

  Uri get origin => Uri.parse('http://127.0.0.1:${_server.port}');

  Future<void> start() async {
    _server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    _server.listen(_handle);
  }

  Future<void> stop() => _server.close(force: true);

  Future<void> _handle(HttpRequest request) async {
    final response = request.response;
    final path = request.uri.path;
    if (request.method == 'GET' && path == '/api/v1/auth/desktop') {
      final q = request.uri.queryParameters;
      expect(q['code_challenge_method'], 'S256');
      expect(q['redirect_uri'], startsWith('http://127.0.0.1:'));
      challenge = q['code_challenge'];
      response
        ..statusCode = HttpStatus.found
        ..headers.set(
          HttpHeaders.locationHeader,
          '${q['redirect_uri']}?code=one-time-code&state=${q['state']}',
        );
    } else if (request.method == 'POST' &&
        path == '/api/v1/auth/desktop/token') {
      final body = jsonDecode(
        await utf8.decoder.bind(request).join(),
      ) as Map<String, dynamic>;
      final sent = base64Url
          .encode(sha256(ascii.encode(body['codeVerifier'] as String)))
          .replaceAll('=', '');
      if (refuseToken ||
          request.headers.value('x-domowik-csrf') == null ||
          body['code'] != 'one-time-code' ||
          sent != challenge) {
        response
          ..statusCode = HttpStatus.unauthorized
          ..write(jsonEncode({'message': 'signing in could not be completed'}));
      } else {
        response
          ..statusCode = HttpStatus.created
          ..write(
            jsonEncode({
              'id': 'token-id',
              'name': 'desktop',
              'token': 'dmk_secret',
              'expiresInSeconds': 2592000,
            }),
          );
      }
    } else if (request.method == 'GET' &&
        (path == '/api/v1/auth/me' || path == '/api/v1/plans')) {
      if (request.headers.value('authorization') != 'Bearer dmk_secret') {
        response.statusCode = HttpStatus.unauthorized;
      } else {
        response.write(
          path == '/api/v1/auth/me'
              ? '{"userId":"u","username":"ann@example.com","planLimit":1}'
              : '[{"id":"p","title":"Flat","revision":"1","storedBytes":10,'
                    '"journalLen":0,"updatedAtEpoch":1793059200}]',
        );
      }
    } else if (request.method == 'DELETE' &&
        path == '/api/v1/tokens/token-id') {
      revoked.add(request.headers.value('authorization')!);
      response.statusCode = HttpStatus.noContent;
    } else {
      response.statusCode = HttpStatus.notFound;
    }
    await response.close();
  }
}

/// A browser that follows the server's redirect to the app's loopback.
Future<void> followingBrowser(Uri url) async {
  final client = HttpClient();
  try {
    final response = await (await client.getUrl(url)).close();
    await response.drain<void>();
  } finally {
    client.close(force: true);
  }
}

void main() {
  test('accepts only an HTTPS origin, or HTTP on loopback', () {
    expect(
      ServerEndpoint('https://planner.example').api('auth/me').toString(),
      'https://planner.example/api/v1/auth/me',
    );
    expect(ServerEndpoint('http://127.0.0.1:8080/').origin.port, 8080);
    for (final url in [
      'http://planner.example',
      'https://planner.example/api',
      'https://planner.example?x=1',
      'https://user@planner.example',
      'ftp://planner.example',
      'planner.example',
    ]) {
      expect(() => ServerEndpoint(url), throwsFormatException, reason: url);
    }
  });

  group('native sign-in', () {
    late FakeServer server;
    setUp(() async {
      server = FakeServer();
      await server.start();
    });
    tearDown(() => server.stop());

    test('signs in through the browser and signs out', () async {
      final client = NativeAccountClient(
        ServerEndpoint(server.origin.toString()),
        openBrowser: followingBrowser,
      );
      final session = (await client.signIn())!;
      expect(session.token, 'dmk_secret');
      expect(session.tokenId, 'token-id');
      final profile = await client.profile(session);
      expect(profile.email, 'ann@example.com');
      expect(profile.planLimit, 1);
      expect(profile.plans.single.title, 'Flat');
      expect(
        profile.plans.single.updatedAt,
        DateTime.fromMillisecondsSinceEpoch(1793059200000),
      );
      await expectLater(
        client.profile(const AccountSession(token: 'stale')),
        throwsA(isA<AccountException>()),
      );
      await client.signOut(session);
      expect(server.revoked, ['Bearer dmk_secret']);
    });

    test('reports a refused code', () async {
      server.refuseToken = true;
      final client = NativeAccountClient(
        ServerEndpoint(server.origin.toString()),
        openBrowser: followingBrowser,
      );
      await expectLater(
        client.signIn(),
        throwsA(
          isA<AccountException>().having(
            (e) => e.message,
            'message',
            contains('signing in could not be completed'),
          ),
        ),
      );
    });

    test(
      'ignores a callback for another sign-in and gives up in time',
      () async {
        final client = NativeAccountClient(
          ServerEndpoint(server.origin.toString()),
          timeout: const Duration(milliseconds: 300),
          openBrowser: (url) async {
            final redirect = Uri.parse(url.queryParameters['redirect_uri']!);
            await followingBrowser(
              redirect.replace(
                queryParameters: {'code': 'x', 'state': 'other'},
              ),
            );
          },
        );
        await expectLater(client.signIn(), throwsA(isA<AccountException>()));
      },
    );

    test('reports a browser that cannot be opened', () async {
      final client = NativeAccountClient(
        ServerEndpoint(server.origin.toString()),
        openBrowser: (_) async =>
            throw const AccountException('Could not open the web browser'),
      );
      await expectLater(client.signIn(), throwsA(isA<AccountException>()));
    });
  });

  for (final scheme in ['http', 'https']) {
    test('an answer cut short over $scheme is reported, not thrown', () async {
      final server = await ServerSocket.bind(InternetAddress.loopbackIPv4, 0);
      server.listen((socket) {
        socket.first.then((_) async {
          socket.write(
            'HTTP/1.1 200 OK\r\nContent-Length: 128\r\n'
            'Connection: close\r\n\r\n{"username":',
          );
          await socket.close();
        });
      });
      addTearDown(server.close);
      final client = NativeAccountClient(
        ServerEndpoint('$scheme://127.0.0.1:${server.port}'),
      );
      await expectLater(
        client.profile(const AccountSession(token: 'dmk_secret')),
        throwsA(isA<AccountException>()),
      );
    });
  }

  test('the desktop server: override, then preference, then the default', () {
    expect(effectiveServerUrl(), defaultServerUrl);
    expect(defaultServerUrl, 'https://planner.homehero.pro');
    expect(effectiveServerUrl(saved: ''), '');
    expect(effectiveServerUrl(saved: 'https://a.example'), 'https://a.example');
    expect(
      effectiveServerUrl(override: '', saved: 'https://a.example'),
      '',
      reason: 'an empty override turns accounts off',
    );
    expect(
      effectiveServerUrl(override: ' https://b.example ', saved: ''),
      'https://b.example',
    );
    expect(
      ServerEndpoint('https://Planner.Example/').canonical,
      ServerEndpoint('https://planner.example').canonical,
    );
  });

  test(
    'the desktop client follows the preference and makes no request',
    () {
      expect(createAccountClient(savedServerUrl: ''), isNull);
      expect(createAccountClient(savedServerUrl: 'ftp://nope'), isNull);
      final client = createAccountClient(
        savedServerUrl: 'https://planner.example',
      )!;
      expect(client.server.canonical, 'https://planner.example');
      expect(client.signInLeavesEditor, isFalse);
      expect(
        createAccountClient()!.server.canonical,
        'https://planner.homehero.pro',
      );
    },
    skip: Platform.environment.containsKey(serverUrlSetting)
        ? '$serverUrlSetting overrides the preference here'
        : false,
  );

  group('desktop plan requests', () {
    late RecordingServer server;
    late NativeAccountClient client;
    const session = AccountSession(token: 'dmk_secret', tokenId: 't1');
    var status = 200;
    var answer = '';
    Map<String, String> headers = {};
    List<int>? bytes;

    setUp(() async {
      status = 200;
      answer = '';
      headers = {};
      bytes = null;
      server = RecordingServer((request, response) async {
        response.statusCode = status;
        headers.forEach(response.headers.set);
        if (bytes != null) {
          response.add(bytes!);
        } else {
          response.write(answer);
        }
      });
      await server.start();
      client = NativeAccountClient(ServerEndpoint(server.origin.toString()));
    });
    tearDown(() => server.stop());

    test('authenticate with the token alone and take api leases', () async {
      answer = '{"leaseId":"l1","revision":"7","expiresInSeconds":90}';
      final lease = await client.acquireLease(session, 'p 1');
      expect(lease.leaseId, 'l1');
      expect(lease.revision, '7');
      expect(lease.mode, 'api');
      final sent = server.received.single;
      expect(sent.method, 'POST');
      expect(sent.path, '/api/v1/plans/p%201/lease');
      expect(sent.headers.value('authorization'), 'Bearer dmk_secret');
      expect(sent.headers.value('x-domowik-csrf'), isNull);
      expect(sent.headers.value('cookie'), isNull);
      expect(sent.headers.value('origin'), isNull);
      expect(sent.headers.contentType?.mimeType, 'application/json');
      expect(jsonDecode(utf8.decode(sent.body)), {'mode': 'api'});
    });

    test('upload, download and save keep revisions exact', () async {
      final package = Uint8List.fromList([1, 2, 3, 0, 255]);
      answer = _plan;
      final imported = await client.importPlan(session, 'My flat', package);
      expect(imported.id, 'p1');
      expect(imported.revision, '7');
      final upload = server.received.last;
      expect(upload.path, '/api/v1/plans/import');
      expect(upload.query, {'title': 'My flat'});
      expect(upload.headers.contentType?.mimeType, 'application/octet-stream');
      expect(upload.body, package);

      bytes = package;
      headers = {'x-domowik-revision': '7'};
      final download = await client.downloadPlan(session, 'p1');
      expect(download.bytes, package);
      expect(download.revision, '7');

      bytes = null;
      headers = {};
      answer = _plan.replaceFirst('"7"', '"8"');
      final saved = await client.savePlanContent(
        session,
        'p1',
        'l1',
        '7',
        package,
      );
      expect(saved.revision, '8');
      final save = server.received.last;
      expect(save.method, 'PUT');
      expect(save.path, '/api/v1/plans/p1/content');
      expect(save.headers.value('x-domowik-lease'), 'l1');
      expect(save.headers.value('if-match'), '"7"');
      expect(save.body, package);

      status = 204;
      answer = '';
      await client.releaseLease(session, 'p1', 'l1');
      expect(jsonDecode(utf8.decode(server.received.last.body)), {
        'leaseId': 'l1',
      });
    });

    test('a download without its revision is refused', () async {
      bytes = [1, 2, 3];
      await expectLater(
        client.downloadPlan(session, 'p1'),
        throwsA(isA<AccountException>()),
      );
    });

    test('refusals keep the status and error code', () async {
      status = 409;
      answer = '{"code":"revision-conflict","message":"changed"}';
      await expectLater(
        client.savePlanContent(session, 'p1', 'l1', '7', Uint8List(1)),
        throwsA(
          isA<AccountException>()
              .having((e) => e.statusCode, 'status', 409)
              .having((e) => e.code, 'code', 'revision-conflict'),
        ),
      );
      status = 409;
      answer = '{"code":"lease-held","message":"held"}';
      await expectLater(
        client.acquireLease(session, 'p1'),
        throwsA(
          isA<AccountException>().having((e) => e.code, 'code', 'lease-held'),
        ),
      );
    });

    test('redirects are refused, not followed', () async {
      status = 302;
      headers = {'location': '${server.origin}/elsewhere'};
      await expectLater(
        client.listPlans(session),
        throwsA(
          isA<AccountException>().having((e) => e.statusCode, 'status', 302),
        ),
      );
      expect(server.received, hasLength(1));
    });

    test('an oversized answer is refused', () async {
      bytes = Uint8List(NativeAccountClient.maxResponseBytes + 1);
      await expectLater(
        client.listPlans(session),
        throwsA(
          isA<AccountException>().having(
            (e) => e.message,
            'message',
            contains('too large'),
          ),
        ),
      );
    });

    test('a slow answer times out', () async {
      final stalled = RecordingServer((request, response) async {
        await Future<void>.delayed(const Duration(seconds: 2));
      });
      await stalled.start();
      addTearDown(stalled.stop);
      await expectLater(
        NativeAccountClient(
          ServerEndpoint(stalled.origin.toString()),
          responseTimeout: const Duration(milliseconds: 200),
        ).listPlans(session),
        throwsA(
          isA<AccountException>().having(
            (e) => e.message,
            'message',
            contains('did not answer'),
          ),
        ),
      );
    });

    test('signing out an expired token still signs out', () async {
      status = 401;
      await client.signOut(session);
      expect(server.received.single.method, 'DELETE');
      expect(server.received.single.path, '/api/v1/tokens/t1');
      status = 500;
      await expectLater(
        client.signOut(session),
        throwsA(isA<AccountException>()),
      );
    });
  });
}

/// One request a [RecordingServer] received.
typedef Received = ({
  String method,
  String path,
  Map<String, String> query,
  HttpHeaders headers,
  List<int> body,
});

/// A server that records each request and answers with [reply].
class RecordingServer {
  RecordingServer(this.reply);

  final Future<void> Function(Received request, HttpResponse response) reply;
  final received = <Received>[];
  late final HttpServer _server;

  Uri get origin => Uri.parse('http://127.0.0.1:${_server.port}');

  Future<void> start() async {
    _server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
    _server.listen((request) async {
      final body = await request.fold<List<int>>(
        [],
        (all, c) => all..addAll(c),
      );
      final r = (
        method: request.method,
        path: request.uri.path,
        query: request.uri.queryParameters,
        headers: request.headers,
        body: body,
      );
      received.add(r);
      await reply(r, request.response);
      await request.response.close();
    });
  }

  Future<void> stop() => _server.close(force: true);
}

const _plan =
    '{"id":"p1","title":"Flat","revision":"7","updatedAtEpoch":1793059200}';
