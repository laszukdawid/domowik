@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/platform/account_web.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('globalThis.fetch')
external JSFunction get _fetch;
@JS('globalThis.fetch')
external set _fetch(JSFunction value);

@JS('Response')
extension type _Response._(JSObject _) implements JSObject {
  external factory _Response(JSAny? body, JSObject init);
}

@JS('document.head.insertAdjacentHTML')
external void _insertHtml(String position, String html);
@JS('document.querySelector')
external _Element? _query(String selector);

extension type _Element(JSObject _) implements JSObject {
  external void remove();
}

/// One request the editor sent.
typedef Sent = ({
  String url,
  String method,
  String? csrf,
  String credentials,
  String redirect,
  String? lease,
  String? ifMatch,
  String? contentType,
  String? jsonBody,
  Uint8List? binaryBody,
});

/// Replaces `fetch` with one answering `status` and `body`, or the body
/// [byPath] gives the URL's path, recording calls.
List<Sent> answer(
  int status,
  String? body, {
  Map<String, String>? byPath,
  Map<String, String>? responseHeaders,
  Uint8List? binaryResponse,
}) {
  final sent = <Sent>[];
  final original = _fetch;
  var pendingBinaryResponse = binaryResponse;
  addTearDown(() => _fetch = original);
  _fetch = ((JSString url, JSObject init) {
    final headers = init['headers'] as JSObject;
    final requestBody = init['body'];
    final isBytes = requestBody is JSUint8Array;
    sent.add((
      url: url.toDart,
      method: (init['method'] as JSString).toDart,
      csrf: (headers['X-Domowik-CSRF'] as JSString?)?.toDart,
      credentials: (init['credentials'] as JSString).toDart,
      redirect: (init['redirect'] as JSString).toDart,
      lease: (headers['X-Domowik-Lease'] as JSString?)?.toDart,
      ifMatch: (headers['If-Match'] as JSString?)?.toDart,
      contentType: (headers['Content-Type'] as JSString?)?.toDart,
      jsonBody: requestBody is JSString ? requestBody.toDart : null,
      binaryBody: isBytes ? requestBody.toDart : null,
    ));
    final reply = byPath?[Uri.parse(url.toDart).path] ?? body;
    final responseBody = pendingBinaryResponse?.toJS ?? reply?.toJS;
    pendingBinaryResponse = null;
    return Future.value(
      _Response(
        responseBody,
        {
              'status': status,
              if (responseHeaders != null) 'headers': responseHeaders,
            }.jsify()!
            as JSObject,
      ),
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

    test(
      'optional hosted library preserves plan IDs and string revisions',
      () async {
        final client = BrowserAccountClient(server);
        final sent = answer(
          200,
          '[{"id":"p-1","title":"Flat","revision":"9007199254740993",'
          '"updatedAtEpoch":10}]',
        );
        final plans = await client.listPlans(
          const AccountSession(token: 'csrf'),
        );
        expect(plans.single.id, 'p-1');
        expect(plans.single.revision, '9007199254740993');
        expect(sent.single.method, 'GET');
        expect(sent.single.csrf, isNull);
      },
    );

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

    test(
      'hosted transport sends same-origin CSRF and browser lease mode',
      () async {
        final client = BrowserAccountClient(server);
        final sent = answer(
          201,
          null,
          byPath: {
            '/api/v1/plans':
                '{"id":"p1","title":"New","revision":"1",'
                '"updatedAtEpoch":10}',
            '/api/v1/plans/import':
                '{"id":"p2","title":"Copy","revision":"3",'
                '"updatedAtEpoch":20}',
            '/api/v1/plans/p1/lease':
                '{"leaseId":"l1","revision":"7","mode":"browser",'
                '"expiresInSeconds":90}',
            '/api/v1/plans/p1/lease/renew':
                '{"leaseId":"l1","revision":"7","mode":"browser",'
                '"expiresInSeconds":90}',
          },
        );
        final account = const AccountSession(token: 'csrf-token');
        final created = await client.createPlan(account, 'New');
        final imported = await client.importPlan(
          account,
          'Copy',
          Uint8List.fromList([4, 5]),
        );
        final lease = await client.acquireLease(account, 'p1');
        await client.renewLease(account, 'p1', lease.leaseId);
        expect(created.id, 'p1');
        expect(created.revision, '1');
        expect(imported.id, 'p2');
        expect(lease.revision, '7');
        expect(sent.map((r) => r.method), ['POST', 'POST', 'POST', 'POST']);
        expect(sent.map((r) => r.csrf), everyElement('csrf-token'));
        expect(sent[0].jsonBody, '{"title":"New"}');
        expect(sent[1].url, '$origin/api/v1/plans/import?title=Copy');
        expect(sent[1].binaryBody, [4, 5]);
        expect(sent[2].jsonBody, '{"mode":"browser"}');
        expect(sent.every((r) => r.credentials == 'same-origin'), isTrue);
        expect(sent.every((r) => r.redirect == 'error'), isTrue);
      },
    );

    test(
      'download revision and conditional hosted save headers are preserved',
      () async {
        final client = BrowserAccountClient(server);
        final sent = answer(
          200,
          '{"id":"p1","title":"Flat","revision":"8",'
          '"updatedAtEpoch":30}',
          responseHeaders: {'X-Domowik-Revision': '7'},
          binaryResponse: Uint8List.fromList([1, 2, 3]),
        );
        final account = const AccountSession(token: 'csrf-token');
        final download = await client.downloadPlan(account, 'p1');
        expect(download.bytes, [1, 2, 3]);
        expect(download.revision, '7');
        final saved = await client.savePlanContent(
          account,
          'p1',
          'lease-9',
          '7',
          Uint8List.fromList([6, 7]),
        );
        expect(saved.revision, '8');
        expect(sent[0].method, 'GET');
        expect(sent[0].csrf, isNull);
        expect(sent[1].method, 'PUT');
        expect(sent[1].csrf, 'csrf-token');
        expect(sent[1].lease, 'lease-9');
        expect(sent[1].ifMatch, '"7"');
        expect(sent[1].binaryBody, [6, 7]);
        expect(sent.every((r) => r.credentials == 'same-origin'), isTrue);
        expect(sent.every((r) => r.redirect == 'error'), isTrue);
      },
    );

    test(
      'lease release uses CSRF and stale writers retain refusal details',
      () async {
        final client = BrowserAccountClient(server);
        final sent = answer(204, null);
        await client.releaseLease(
          const AccountSession(token: 'csrf-token'),
          'p1',
          'lease-9',
        );
        expect(sent.single.method, 'POST');
        expect(sent.single.csrf, 'csrf-token');
        expect(sent.single.jsonBody, '{"leaseId":"lease-9"}');

        answer(409, '{"code":"revision-conflict","message":"stale"}');
        await expectLater(
          client.savePlanContent(
            const AccountSession(token: 'csrf-token'),
            'p1',
            'lease-9',
            '7',
            Uint8List.fromList([6]),
          ),
          throwsA(
            isA<AccountException>()
                .having((error) => error.statusCode, 'status', 409)
                .having((error) => error.code, 'code', 'revision-conflict'),
          ),
        );
      },
    );
  });
}
