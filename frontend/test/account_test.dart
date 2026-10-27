import 'dart:convert';
import 'dart:io';

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
}
