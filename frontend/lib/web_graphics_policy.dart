/// Browser-only 3D admission, decided without touching a GPU API.
///
/// This is a conservative compatibility policy, not a driver capability test.
/// A native browser/driver crash cannot be caught by Dart or JavaScript. URL
/// overrides affect this page only, never the plan or stored view preferences.
class WebGraphicsPolicy {
  const WebGraphicsPolicy.allowed() : disabledReason = null;
  const WebGraphicsPolicy.safe2d(String reason) : disabledReason = reason;

  factory WebGraphicsPolicy.forBrowser({
    required String userAgent,
    required Uri page,
  }) {
    switch (page.queryParameters['DOMOWIK_WEBGPU']) {
      case '0':
        return const WebGraphicsPolicy.safe2d(
          'Safe 2D: 3D is disabled by this page URL. '
          'You can continue editing and saving your plan in 2D.',
        );
      case '1':
        return const WebGraphicsPolicy.allowed();
    }
    if (userAgent.contains('Firefox/') &&
        userAgent.contains('Linux') &&
        !userAgent.contains('Android')) {
      return const WebGraphicsPolicy.safe2d(
        'Safe 2D: 3D is turned off as a precaution. Some Firefox on Linux '
        'graphics configurations can crash the browser when 3D starts. '
        'You can continue editing and saving your plan in 2D. '
        'Your saved view preferences have not been changed.',
      );
    }
    return const WebGraphicsPolicy.allowed();
  }

  final String? disabledReason;
  bool get allows3d => disabledReason == null;

  /// The requested preference remains unchanged when the visible mode falls back.
  String viewMode(String requested) => allows3d ? requested : '2d';
}
