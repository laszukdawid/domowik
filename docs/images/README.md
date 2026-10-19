# Documentation images

These images use the original [Welcome Home example](../../examples/welcome-home/README.md).
They contain no owner plans or third-party model/catalog content and are
licensed with the project under Apache-2.0. Existing project notices for
platform scaffolding and fonts still apply.

- `welcome-home-editor.png`: actual Linux release editor, 2026-10-18, showing
  the included `welcome-home.domo` in **2D + 3D split view**, default Orbit
  camera. Captured at 1600 × 1000 on an isolated X11 display with software GL;
  only the bottom status strip was cropped. It is functional illustration,
  not a hardware performance claim. No scene pixels or UI were retouched.
- `welcome-home-library.png`: the same app and document, **Furniture**, search
  `sofa`. Cropped to the dialog and a little surrounding context. The displayed
  source path is relative provenance; the model bytes are embedded.
- `welcome-home-plan.png`: actual CLI `export-png --scale 0.12`, regenerated
  by the example script. Shows room labels/areas, walls and openings. Current
  CLI exports do not draw imported furniture, unlike the editor.

To refresh the screenshots, build/run the Linux editor, open the example,
resize to 1600 × 1000, select **2D + 3D split view**, and capture the window.
The default fitted camera is used. For the library shot, open **Furniture**
and enter `sofa`. Keep paths or personal data out of the frame. Screenshots
are maintained separately from the generator because they show the real app.
