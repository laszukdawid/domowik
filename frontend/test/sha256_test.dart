import 'dart:convert';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/sha256.dart';
import 'package:flutter_test/flutter_test.dart';

String hex(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

void main() {
  test('matches the FIPS 180-4 example vectors', () {
    expect(
      hex(sha256(const [])),
      'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    );
    expect(
      hex(sha256(ascii.encode('abc'))),
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
    expect(
      hex(
        sha256(
          ascii.encode(
            'abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq',
          ),
        ),
      ),
      '248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1',
    );
    expect(
      hex(sha256(List<int>.filled(1000000, 0x61))),
      'cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0',
    );
  });

  test('derives the RFC 7636 example challenge', () {
    const verifier = 'dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk';
    expect(
      base64Url.encode(sha256(ascii.encode(verifier))).replaceAll('=', ''),
      'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
    );
    final pkce = Pkce.create();
    expect(pkce.verifier.length, 43);
    expect(pkce.challenge.length, 43);
  });
}
