# Resize dialog layout and preview — issue 0083

Original user direction on 2026-10-04: limit title length and width, display
millimetres to one decimal, and preview resizing while editing.

The modal is capped at 420 logical pixels and adapts to a 390-pixel window.
Names longer than 50 characters use the first 49 plus an ellipsis; the full
name is a hover tooltip. Numeric text is formatted to one decimal initially,
after proportional calculations, on blur and for pasted extra precision.
Untouched underlying dimensions retain their imported precision, so opening
and applying does not quantize the document.

The preview borrows cached top-view artwork from the editor. It paints the
proposed footprint against its original dashed outline and compares original
and proposed height with a separate bar. It is a size comparison, not a new 3D
rendering surface. Missing art uses an explicitly labelled footprint. Valid
changes redraw immediately; invalid input retains the last valid preview and
disables Apply. It creates no native operations or temporary plan state.

Regression checks cover the width cap, long-name tooltip on actual mouse hover,
decimal formatting, linked values, rendered preview pixels changing, invalid
preview retention, untouched precision on Apply, Cancel and a narrow window.
Existing native-backed resize/history/persistence tests continue to pass.
A temporary widget-render screenshot was visually inspected using an installed
system font; no font or screenshot was added to distributed assets or fixtures.

No new dependencies, source artwork, geometry algorithms or format changes.
The cached object image remains owned by the editor, not the dialog; dimensions
and preview painting are local display state. Canonical verification and the
Linux release build are recorded at closure in the tracker.
