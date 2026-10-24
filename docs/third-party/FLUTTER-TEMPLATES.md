# Flutter template files and launcher artwork

The committed Flutter-generated platform scaffolding and default launcher
icons in `frontend/` originate in the Flutter
SDK templates, not in Domowik's independent implementation. This includes
Android manifest/resources/icons, macOS runner/project/icon scaffolding and
Linux runner/build scaffolding. Domowik modifications remain Apache-2.0;
upstream template content retains its BSD-3-Clause notice and terms.

The complete upstream copyright, conditions and disclaimer are retained in
[notices/flutter/LICENSE](notices/flutter/LICENSE). This notice applies to
source publication too, not only to shipping a compiled application.

## Provenance

- Upstream: https://github.com/flutter/flutter
- Template tree: https://github.com/flutter/flutter/tree/3.41.6/packages/flutter_tools/templates
- License: https://github.com/flutter/flutter/blob/3.41.6/LICENSE
- Checked 2026-10-04 against the installed SDK at revision
  `6a19cca56475dbfba1478ee68d7bd0c2ef891da1` and the upstream 3.41.6 license.
  The retained text is copied unchanged from the installed SDK's LICENSE.
- Rechecked 2026-10-20 when CI moved to Flutter 3.47.7 (revision
  `abaf9c523780a608bd46686fd5e53740a07077f8`): its LICENSE is byte-identical
  to the retained copy. Generated platform files were not regenerated; the
  3.41.6 template references above remain their provenance.
- Earlier SDK use is recorded in `tools.json`; Android generation is recorded
  in `toolchain-android.json`. The 3.41.6 reference is not a claim that every
  historical generated file came from that version.

This closes the missing retained source notice, not the staged GUI binary
inventory. Flutter engine/Dart dependencies, framework/font licenses and
platform libraries in downloadable bundles still require artifact-specific
review under [RELEASE-INVENTORY.md](../RELEASE-INVENTORY.md).
Owner-imported models in ignored `assets/` are not Flutter template content and
are not included or relicensed by this record.
