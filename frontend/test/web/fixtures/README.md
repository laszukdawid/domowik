# Browser test fixtures

- `optional-missing-model.domo`: a valid thin plan on Level 1 with one
  4,000 mm model instance whose optional model blob is absent, so the
  instance opens as a placeholder. Made from a plan this editor encoded
  (`domowik_plan_encode`): the `blob/` entry was removed from the archive,
  its inventory row dropped and the profile set to `thin`. The model it
  lacks is the one-triangle OBJ `v 0 0 0 / v 2 0 0 / v 0 1 2 / f 1 2 3`.
  Used by `three_d_web_test.dart` (issue 0088 review round 1).
