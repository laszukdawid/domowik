# Asset directory chooser and path editing — issue 0075

## Workflow

**Assets → Add directory** opens the system directory chooser. Select a folder
to attach it immediately; Cancel leaves the library unchanged. Linux uses
GTK's local-folder chooser, macOS uses AppKit's directory-only NSOpenPanel.

**Enter path** opens an optional manual path field. Submit it with Enter or the
checkmark. Already attached paths are listed above the models: the pencil edits
the path, and the folder icon chooses a replacement using the current location
as the initial directory. Replacement validates before changing the plan,
deduplicates an already attached destination, and keeps embedded models and
placed instances intact. Cancel path edit discards the text draft.

Picker failures offer manual entry. While a picker is pending, library actions
are disabled; a late response after unmount cannot attach a directory. This
increment supports native directory selection on the packaged desktop targets
Linux/macOS. Android provider URI access requires its own library-I/O adapter;
it is not treated as a filesystem path here.

## Provenance and compatibility

Original owner-requested UX, implemented with existing GTK/AppKit SDKs and the
existing method channel; no new packages or external assets. GTK selection is
asynchronous on its main event loop, retaining the method call until the
response. The channel holds a weak window reference rather than a window/engine
ownership cycle. AppKit uses the existing synchronous modal-panel convention.
Cancellation returns null on both platforms.

Manifest metadata keeps the existing optional `editorAssetDirectories` array.
The native asset API gains the `replaceDirectory` request kind without changing
existing request layouts or the ABI 15 export surface. Older implementations
reject the unfamiliar kind instead of silently adding a second directory.

## Verification

Native tests cover rejected, unchanged, valid, duplicate and stale replacements,
embedded-content preservation and save/reopen. Flutter tests cover system-picker
dispatch, cancellation, duplicate picks, pending request guards, errors, manual
entry, replacement through both typing and browsing, initial path forwarding and
late responses after unmount. The complete native-backed placement/persistence
test now adds its directory through the picker channel.

`flutter analyze`, the five focused Flutter tests and `flutter build linux
--release` passed on the local Linux host. macOS picker source was updated but
not executed on this Linux host. No interactive native-dialog execution is
claimed by the mocked method-channel widget tests.

The complete Flutter suite passed **268 tests**. Canonical `./scripts/verify.sh`
reached its final clean-tree assertion after all preceding executed stages
passed, then printed **`=== VERIFICATION FAILED ===`**, **`STAGE: P13 clean tree`**,
exit **1**, because pre-existing work and this implementation are uncommitted.
No full verification pass is claimed.
