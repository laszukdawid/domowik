import 'package:domowik_editor/web_graphics_policy.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const linuxFirefox =
      'Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:159.0) '
      'Gecko/20100101 Firefox/159.0';
  const macFirefox =
      'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:159.0) '
      'Gecko/20100101 Firefox/159.0';
  WebGraphicsPolicy policy(String ua, [String query = '']) =>
      WebGraphicsPolicy.forBrowser(
        userAgent: ua,
        page: Uri.parse('https://editor.invalid/$query'),
      );

  test('desktop Linux Firefox defaults to 2D without a GPU probe', () {
    final safe = policy(linuxFirefox);
    expect(safe.allows3d, isFalse);
    expect(safe.disabledReason, contains('Firefox on Linux'));
    expect(safe.disabledReason, contains('as a precaution'));
    expect(safe.disabledReason, contains('Some Firefox on Linux'));
    for (final mode in ['2d', '3d', 'split']) {
      expect(safe.viewMode(mode), '2d');
    }
  });

  test('macOS Firefox and other browsers keep their existing 3D admission', () {
    for (final ua in [
      macFirefox,
      'Mozilla/5.0 (Windows NT 10.0) Gecko/20100101 Firefox/159.0',
      'Mozilla/5.0 (X11; Linux x86_64) Chrome/155.0.0.0 Safari/537.36',
      'Mozilla/5.0 (Android 16; Mobile; Linux) Gecko/159.0 Firefox/159.0',
      '',
    ]) {
      expect(policy(ua).allows3d, isTrue, reason: ua);
      expect(policy(ua).viewMode('split'), 'split');
    }
  });

  test('explicit safe mode wins on every browser', () {
    for (final ua in [linuxFirefox, macFirefox, '']) {
      final safe = policy(ua, '?DOMOWIK_WEBGPU=0');
      expect(safe.allows3d, isFalse);
      expect(safe.disabledReason, contains('disabled by this page URL'));
    }
  });

  test('only an explicit 1 opts Linux Firefox into experimental 3D', () {
    expect(policy(linuxFirefox, '?DOMOWIK_WEBGPU=1').allows3d, isTrue);
    for (final value in ['', 'true', 'yes', '2']) {
      expect(policy(linuxFirefox, '?DOMOWIK_WEBGPU=$value').allows3d, isFalse);
    }
  });
}
