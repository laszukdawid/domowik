@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/platform/account_web.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('globalThis.fetch')
external JSFunction get _fetch;
@JS('globalThis.fetch')
external set _fetch(JSFunction value);

@JS('Response')
extension type _Response._(JSObject _) implements JSObject {
  external factory _Response(String? body, JSObject init);
}

@JS('document.head.insertAdjacentHTML')
external void _insertHtml(String position, String html);
@JS('document.querySelector')
external _Element? _query(String selector);

extension type _Element(JSObject _) implements JSObject {
  external void remove();
}

/// One request the editor sent.
typedef Sent = ({String url, String method, String? csrf, String credentials});

/// Replaces `fetch` with one answering `status` and `body`, or the body
/// [byPath] gives the URL's path, recording calls.
List<Sent> answer(int status, String? body, {Map<String, String>? byPath}) {
  final sent = <Sent>[];
  final original = _fetch;
  addTearDown(() => _fetch = original);
  _fetch = ((JSString url, JSObject init) {
    final headers = init['headers'] as JSObject;
    sent.add((
      url: url.toDart,
      method: (init['method'] as JSString).toDart,
      csrf: (headers['X-Domowik-CSRF'] as JSString?)?.toDart,
      credentials: (init['credentials'] as JSString).toDart,
    ));
    final reply = byPath?[Uri.parse(url.toDart).path] ?? body;
    return Future.value(
      _Response(reply, {'status': status}.jsify()! as JSObject),
    ).toJS;
  }).toJS;
  return sent;
}

void main() {
  final origin = Uri.base.origin;

  test('signing in is off unless the page names its own origin', () {
    void removeMeta() => _query('meta[name="domowik-server-url"]')?.remove();
    addTearDown(removeMeta);
    expect(createAccountClient(), isNull);

    _insertHtml('beforeend', '<meta name="domowik-server-url" content="/">');
    expect(createAccountClient()!.server.origin.origin, origin);
    removeMeta();

    _insertHtml(
      'beforeend',
      '<meta name="domowik-server-url" content="https://other.example">',
    );
    expect(createAccountClient(), isNull);
    removeMeta();

    _insertHtml('beforeend', '<meta name="domowik-server-url" content="">');
    expect(createAccountClient(), isNull);
  });

  group('browser session', () {
    final server = ServerEndpoint(origin);

    test('restores a signed-in session, or none', () async {
      final client = BrowserAccountClient(server);
      final sent = answer(200, '{"userId":"u","username":"a","csrfToken":"c"}');
      final session = await client.restore();
      expect(session!.token, 'c');
      expect(sent.single.url, '$origin/api/v1/auth/session');
      expect(sent.single.credentials, 'same-origin');

      answer(401, '{"code":"unauthenticated","message":"sign in"}');
      expect(await client.restore(), isNull);
    });

    test('sends the browser to the server sign-in', () async {
      final visited = <String>[];
      final client = BrowserAccountClient(server, navigate: visited.add);
      final sent = answer(
        200,
        '{"authorizationUrl":"https://idp.example/authorize?state=s"}',
      );
      expect(await client.signIn(), isNull);
      expect(visited, ['https://idp.example/authorize?state=s']);
      expect(sent.single.method, 'POST');
      expect(sent.single.url, '$origin/api/v1/auth/sign-in');
      expect(sent.single.csrf, isNotNull);

      answer(503, '{"code":"sign-in-unavailable","message":"not configured"}');
      await expectLater(
        client.signIn(),
        throwsA(
          isA<AccountException>().having(
            (e) => e.message,
            'message',
            contains('not configured'),
          ),
        ),
      );
    });

    test('reads the account and its plans', () async {
      final client = BrowserAccountClient(server);
      final sent = answer(
        200,
        null,
        byPath: {
          '/api/v1/auth/me':
              '{"userId":"u","username":"ann@example.com","planLimit":1}',
          '/api/v1/plans': '[{"id":"p","title":"Flat","updatedAtEpoch":0}]',
        },
      );
      final profile = await client.profile(const AccountSession(token: 'c'));
      expect(profile.email, 'ann@example.com');
      expect(profile.planLimit, 1);
      expect(profile.plans.single.title, 'Flat');
      expect(sent.map((s) => s.url), {
        '$origin/api/v1/plans',
        '$origin/api/v1/auth/me',
      });
      expect(sent.every((s) => s.method == 'GET' && s.csrf == null), isTrue);

      answer(401, '{"code":"unauthenticated","message":"sign in"}');
      await expectLater(
        client.profile(const AccountSession(token: 'c')),
        throwsA(isA<AccountException>()),
      );
    });

    test('signs out with the session CSRF token', () async {
      final client = BrowserAccountClient(server);
      final sent = answer(204, null);
      await client.signOut(const AccountSession(token: 'c'));
      expect(sent.single.method, 'POST');
      expect(sent.single.url, '$origin/api/v1/auth/logout');
      expect(sent.single.csrf, 'c');

      answer(403, '{"code":"csrf-mismatch","message":"wrong token"}');
      await expectLater(
        client.signOut(const AccountSession(token: 'stale')),
        throwsA(isA<AccountException>()),
      );
    });
  });
}
