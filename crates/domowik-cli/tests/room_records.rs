//! Room records end to end: the CLI, and survival across every boundary
//! BEHAVIOR-SPEC §5.2 says a record has to cross.
//!
//! The acceptance criterion is "records survive save, reopen, undo, redo,
//! compaction and external batch admission, with tests for each", and each of
//! those is a genuinely different mechanism: save/reopen goes through the
//! container, undo and redo through the session stacks, compaction folds the
//! log into the snapshot, and external admission arrives through the log's
//! canonical ordering. A record that survived four of the five would look
//! fine until the fifth.

use domowik_container::{read_package, Package, Plan};
use domowik_document::operation::{Batch, Operation};
use domowik_document::session::Session;
use domowik_edit::{
    create_room_operations, delete_room_operations, update_room_operations, FieldEdit, RoomFields,
};
use domowik_format::{
    Document, Entity, JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin,
};
use domowik_geometry::rooms::{self, RoomStatus};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Output};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// BEHAVIOR-SPEC §5.4's base arrangement B4, as a document.
fn b4() -> (Document, Uuid) {
    let mut doc = Document::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    doc.levels.push(level);
    let corners = [
        (0.0, 0.0),
        (4000.0, 0.0),
        (4000.0, 3000.0),
        (0.0, 3000.0),
        (0.0, 0.0),
    ];
    let mut ids = Vec::new();
    for pair in corners.windows(2) {
        let wall = Wall::new(
            level_id,
            Point::new(pair[0].0, pair[0].1),
            Point::new(pair[1].0, pair[1].1),
            200.0,
            2500.0,
        );
        ids.push(wall.id);
        doc.walls.push(wall);
    }
    for i in 0..4 {
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(ids[i], Role::End),
            JoinMember::new(ids[(i + 1) % 4], Role::Start),
        ]));
    }
    (doc, level_id)
}

fn living(level: Uuid) -> RoomRecord {
    let mut r = RoomRecord::new(Uuid::from_u128(1), level, Point::new(2000.0, 1500.0));
    r.name = Some("Living".to_owned());
    r.floor_style_ref = Some("F-wood".to_owned());
    r
}

/// A session whose baseline snapshot is `doc` and whose log is empty.
fn open_on(doc: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], Vec::new())
        .expect("an empty log over a valid snapshot opens")
}

/// A session on B4 that already holds the `Living` record.
fn session_with_record() -> (Session, Uuid, Uuid) {
    let (doc, level) = b4();
    let mut session = open_on(doc);
    let record = living(level);
    let id = record.id;
    let ops = create_room_operations(session.document(), record).expect("create");
    session.commit(ops).expect("commits");
    (session, level, id)
}

/// Runs `domowik <command> <path> <args...>`.
fn run(path: &Path, command: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_domowik"))
        .arg(command)
        .arg(path)
        .args(args)
        .output()
        .expect("CLI starts")
}

/// Runs `domowik room <sub> <path> <args...>`.
///
/// The path sits AFTER the subcommand for a nested command group, which is
/// the one difference from [`run`] and the reason this is a second helper
/// rather than an extra argument.
fn run_room(path: &Path, sub: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_domowik"))
        .arg("room")
        .arg(sub)
        .arg(path)
        .args(args)
        .output()
        .expect("CLI starts")
}

fn check(out: Output, what: &str) -> String {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "{what} wrote to stderr on success: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn accepted(path: &Path, command: &str, args: &[&str]) -> String {
    check(run(path, command, args), command)
}

fn room(path: &Path, sub: &str, args: &[&str]) -> String {
    check(run_room(path, sub, args), sub)
}

/// A plan on disk with B4's walls, built through the real binary.
fn plan_on_disk(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let path = dir.path().join("plan.domo");
    accepted(&path, "new", &[]);
    for wall in [
        ["0", "0", "4000", "0"],
        ["4000", "0", "4000", "3000"],
        ["4000", "3000", "0", "3000"],
        ["0", "3000", "0", "0"],
    ] {
        accepted(
            &path,
            "add-wall",
            &[wall[0], wall[1], wall[2], wall[3], "--thickness", "200"],
        );
    }
    path
}

/// The one live record of a plan on disk.
fn only_record(path: &Path) -> RoomRecord {
    let pkg = read_package(path).expect("reads");
    let doc = pkg.project().expect("projects").document;
    assert_eq!(doc.room_records.len(), 1, "expected exactly one record");
    doc.room_records[0].clone()
}

// ---------------------------------------------------------------------------
// The CLI surface
// ---------------------------------------------------------------------------

#[test]
fn the_cli_creates_names_restyles_lists_and_deletes_a_record() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);

    let created = room(&path, "add", &["2000", "1500", "--name", "Living"]);
    assert!(created.contains("associated"), "got {created}");

    let listed = room(&path, "list", &[]);
    assert!(listed.contains("\"Living\""), "got {listed}");
    assert!(listed.contains("10.64 m²"), "got {listed}");

    room(
        &path,
        "set",
        &["1", "--name", "Salon", "--floor-style", "F-wood"],
    );
    let record = only_record(&path);
    assert_eq!(record.name.as_deref(), Some("Salon"));
    assert_eq!(record.floor_style_ref.as_deref(), Some("F-wood"));

    // `info` reports the name alongside the area it already reported.
    let info = accepted(&path, "info", &[]);
    assert!(info.contains("\"Salon\""), "got {info}");
    assert!(info.contains("10.64 m²"), "got {info}");
    assert!(info.contains("room records: 1"), "got {info}");

    room(&path, "remove", &["1"]);
    let pkg = read_package(&path).expect("reads");
    let doc = pkg.project().expect("projects").document;
    assert!(doc.room_records.is_empty(), "the record is gone");
    // R5.1.8: the FACE is not.
    assert_eq!(
        rooms::project_level(&doc, doc.levels[0].id).faces.len(),
        1,
        "deleting a record must not delete the room"
    );
}

#[test]
fn a_record_is_addressable_by_its_uuid_as_well_as_its_number() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    room(&path, "add", &["2000", "1500", "--name", "Living"]);
    let id = only_record(&path).id.to_string();
    room(&path, "set", &[&id, "--name", "Kuchnia"]);
    assert_eq!(only_record(&path).name.as_deref(), Some("Kuchnia"));
}

#[test]
fn the_cli_reports_each_non_associating_status() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    // F32's three cases, plus the tie-break loser.
    for (x, y, expected) in [
        ("2000", "100", "seed-on-boundary"),
        ("2000", "0", "seed-in-wall"),
        ("10000", "1500", "seed-in-exterior"),
    ] {
        let out = room(&path, "add", &[x, y, "--name", expected]);
        assert!(out.contains(expected), "({x},{y}) reported {out}");
    }
    let out = room(&path, "add", &["2500", "1500", "--name", "Second"]);
    assert!(
        out.contains("associated"),
        "the first candidate wins: {out}"
    );
    let out = room(&path, "add", &["3000", "1500", "--name", "Third"]);
    assert!(out.contains("superseded"), "a later UUID loses: {out}");
}

#[test]
fn the_json_report_separates_faces_from_records() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    room(&path, "add", &["2000", "1500", "--name", "Living"]);
    room(&path, "add", &["10000", "1500"]);

    let report: serde_json::Value =
        serde_json::from_str(&accepted(&path, "info", &["--json"])).expect("json");
    let level = &report["levels"][0];
    assert_eq!(level["rooms"].as_array().expect("rooms").len(), 1);
    assert_eq!(level["roomRecords"].as_array().expect("records").len(), 2);
    assert_eq!(level["rooms"][0]["label"], "Living");
    assert_eq!(level["rooms"][0]["displayAreaM2"], "10.64");
    assert!(level["rooms"][0]["recordId"].is_string());
    assert_eq!(level["roomRecords"][1]["status"], "seed-in-exterior");
    // An absent name stays absent in the report: `null` and `""` are different
    // authoritative values and only the DISPLAY collapses them.
    assert!(level["roomRecords"][1]["name"].is_null());
}

#[test]
fn f38_a_level_holding_a_record_cannot_be_deleted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    accepted(&path, "new", &[]);
    accepted(&path, "add-level", &["Upstairs"]);
    room(&path, "add", &["0", "0", "--level", "Upstairs"]);

    let out = run(&path, "remove-level", &["Upstairs"]);
    assert!(!out.status.success(), "the deletion must be refused");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("room record(s)"),
        "the refusal must name room records among the blocking kinds: {stderr}"
    );

    // The control: the record moves off the level, and then it deletes.
    room(&path, "set", &["1", "--level", "Level 1"]);
    accepted(&path, "remove-level", &["Upstairs"]);
}

#[test]
fn the_style_and_seed_refusals_reach_the_command_line_with_their_diagnostics() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    room(&path, "add", &["2000", "1500"]);
    let before = std::fs::read(&path).expect("readable");

    for (args, diagnostic) in [
        (
            vec!["set", "1", "--ceiling-style", ""],
            "invalid-style-reference",
        ),
        (vec!["set", "1", "--seed", "nan,0"], "nonfinite-coordinate"),
    ] {
        let out = run_room(&path, args[0], &args[1..]);
        assert!(!out.status.success(), "{args:?} must be refused");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(diagnostic), "{args:?} said {stderr}");
        assert!(out.stdout.is_empty(), "a refusal writes nothing to stdout");
    }
    assert_eq!(
        std::fs::read(&path).expect("readable"),
        before,
        "a refused command must leave the file byte-identical"
    );
}

// ---------------------------------------------------------------------------
// Survival: save, reopen, undo, redo, compaction, external admission
// ---------------------------------------------------------------------------

#[test]
fn a_record_survives_save_and_reopen_with_every_field_intact() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let (doc, level) = b4();
    let mut plan = Plan::unsaved(open_on(doc));
    let mut record = living(level);
    record.ceiling_style_ref = Some("C-white".to_owned());
    let expected = record.clone();
    let ops = create_room_operations(plan.document(), record).expect("create");
    plan.session()
        .expect("editable")
        .commit(ops)
        .expect("commits");
    plan.save_as(&path).expect("saves");

    let reopened = read_package(&path).expect("reads");
    let doc = reopened.project().expect("projects").document;
    assert_eq!(doc.room_records, vec![expected], "every field, unchanged");
    // And it still associates, which is projection rather than stored state.
    let projected = rooms::project_level(&doc, level);
    assert_eq!(
        projected
            .record(Uuid::from_u128(1))
            .expect("present")
            .status,
        RoomStatus::Associated
    );
}

#[test]
fn an_empty_name_survives_a_round_trip_without_becoming_absent() {
    // F37d's distinction is only worth having if the container preserves it.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let (doc, level) = b4();
    let mut record = RoomRecord::new(Uuid::from_u128(1), level, Point::new(2000.0, 1500.0));
    record.name = Some(String::new());
    let mut snapshot = doc;
    snapshot.room_records.push(record);
    domowik_container::write_package(&path, &Package::new(snapshot)).expect("writes");

    let back = read_package(&path).expect("reads");
    assert_eq!(
        back.snapshot.room_records[0].name.as_deref(),
        Some(""),
        "an empty name must not round-trip into an absent one"
    );
    assert_eq!(back.snapshot.room_records[0].display_name(), "Room");
}

#[test]
fn a_record_survives_undo_and_redo_of_every_command_kind() {
    let (mut session, level, id) = session_with_record();
    let created = session.document().room_record(id).expect("created").clone();

    // CREATE: undo removes it, redo brings the SAME id back (R8.1.3).
    session.undo().expect("undo").expect("an entry");
    assert!(session.document().room_record(id).is_none());
    session.redo().expect("redo").expect("an entry");
    assert_eq!(session.document().room_record(id), Some(&created));

    // UPDATE: undo restores the prior field exactly, redo reapplies it.
    let rename = RoomFields {
        name: FieldEdit::Set("Salon".to_owned()),
        ..RoomFields::default()
    };
    let ops = update_room_operations(session.document(), id, &rename).expect("update");
    session.commit(ops).expect("commits");
    assert_eq!(
        session
            .document()
            .room_record(id)
            .expect("live")
            .name
            .as_deref(),
        Some("Salon")
    );
    session.undo().expect("undo").expect("an entry");
    assert_eq!(
        session
            .document()
            .room_record(id)
            .expect("live")
            .name
            .as_deref(),
        Some("Living"),
        "undo restores the prior name, not the fallback"
    );
    session.redo().expect("redo").expect("an entry");
    assert_eq!(
        session
            .document()
            .room_record(id)
            .expect("live")
            .name
            .as_deref(),
        Some("Salon")
    );

    // DELETE: undo recreates an equivalent record (F37j/R5.2.7), redo removes
    // it again. The face is untouched throughout.
    let before_delete = session.document().room_record(id).expect("live").clone();
    let ops = delete_room_operations(session.document(), id).expect("delete");
    session.commit(ops).expect("commits");
    assert!(session.document().room_record(id).is_none());
    assert_eq!(
        rooms::project_level(session.document(), level).faces.len(),
        1,
        "faces exist without records"
    );
    session.undo().expect("undo").expect("an entry");
    assert_eq!(session.document().room_record(id), Some(&before_delete));
    session.redo().expect("redo").expect("an entry");
    assert!(session.document().room_record(id).is_none());
}

#[test]
fn f34_no_wall_edit_resurrects_a_deleted_record() {
    let (mut session, level, id) = session_with_record();
    let ops = delete_room_operations(session.document(), id).expect("delete");
    session.commit(ops).expect("commits");

    // Step 2: move a wall against the now record-less state.
    let wall = session.document().walls[0].id;
    let edit = domowik_edit::set_wall_operations(
        session.document(),
        wall,
        Some(Point::new(0.0, -500.0)),
        Some(Point::new(4000.0, -500.0)),
        None,
        None,
    )
    .expect("moves");
    session.commit(edit.operations).expect("commits");
    assert!(
        session.document().room_records.is_empty(),
        "a wall edit emits ZERO room operations and resurrects nothing"
    );

    // Step 3: undo the wall move. Still no record.
    session.undo().expect("undo").expect("an entry");
    assert!(session.document().room_records.is_empty());

    // Step 4: undo the deletion ITSELF. Now it is back, and associating.
    session.undo().expect("undo").expect("an entry");
    let restored = session.document().room_record(id).expect("restored");
    assert_eq!(restored.name.as_deref(), Some("Living"));
    assert_eq!(
        rooms::project_level(session.document(), level)
            .record(id)
            .expect("projected")
            .status,
        RoomStatus::Associated
    );
}

#[test]
fn a_record_survives_compaction() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    room(
        &path,
        "add",
        &[
            "2000",
            "1500",
            "--name",
            "Living",
            "--floor-style",
            "F-wood",
        ],
    );
    room(&path, "set", &["1", "--ceiling-style", "C-white"]);
    let before = only_record(&path);
    assert!(
        !read_package(&path).expect("reads").batches.is_empty(),
        "the history this test folds away must exist first"
    );

    // `--assert-local-only` is required to execute a fold: FORMAT-SPEC §5.1 S6
    // forbids dropping a batch a peer might still need, and nothing in a
    // package says whether one holds a copy (issue 0027).
    accepted(&path, "compact", &["--assert-local-only"]);

    let after = read_package(&path).expect("reads");
    assert!(after.batches.is_empty(), "compaction folded the log");
    assert_eq!(
        after.snapshot.room_records,
        vec![before],
        "the record must land in the snapshot, not vanish with the log"
    );
    // And it is still the face's effective record afterwards.
    let level = after.snapshot.levels[0].id;
    let projected = rooms::project_level(&after.snapshot, level);
    assert_eq!(
        projected.effective_record(0),
        Some(after.snapshot.room_records[0].id)
    );
}

#[test]
fn a_record_arrives_and_updates_through_external_batch_admission() {
    let (doc, level) = b4();
    let mut session = open_on(doc);

    // Another actor creates the record. R8.3.1's fence and the log's canonical
    // ordering are what this crosses; nothing about rooms is special to it,
    // which is the point worth asserting.
    let other = Uuid::from_u128(0xAA);
    let record = living(level);
    let id = record.id;
    let create = Batch::new(
        other,
        1,
        BTreeMap::new(),
        vec![Operation::create(Entity::RoomRecord(record))],
    );
    session.accept_external(create).expect("admitted");
    assert_eq!(
        session
            .document()
            .room_record(id)
            .expect("arrived")
            .name
            .as_deref(),
        Some("Living")
    );
    assert_eq!(
        rooms::project_level(session.document(), level)
            .record(id)
            .expect("projected")
            .status,
        RoomStatus::Associated,
        "an externally created record projects like any other"
    );

    // And the same actor renames it in a second batch.
    let mut frontier = BTreeMap::new();
    frontier.insert(other, 1);
    let mut fields = serde_json::Map::new();
    fields.insert("name".to_owned(), serde_json::json!("Salon"));
    let rename = Batch::new(other, 2, frontier, vec![Operation::set_fields(id, fields)]);
    session.accept_external(rename).expect("admitted");
    assert_eq!(
        session
            .document()
            .room_record(id)
            .expect("live")
            .name
            .as_deref(),
        Some("Salon")
    );
}

#[test]
fn an_external_batch_is_applied_through_the_apply_command_too() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    let level = read_package(&path).expect("reads").snapshot.levels[0].id;

    let record = living(level);
    let id = record.id;
    let batch = Batch::new(
        Uuid::from_u128(0xBB),
        1,
        BTreeMap::new(),
        vec![Operation::create(Entity::RoomRecord(record))],
    );
    let batch_path = dir.path().join("batch.json");
    std::fs::write(&batch_path, serde_json::to_vec(&batch).expect("serialises")).expect("writes");

    accepted(&path, "apply", &[batch_path.to_str().expect("utf-8")]);
    let landed = only_record(&path);
    assert_eq!(landed.id, id);
    assert_eq!(landed.name.as_deref(), Some("Living"));
}

/// R4.4 through `apply` (Codex GPT-6.1-Sol): a raw batch creating an
/// opening whose id collides with the level, whose host is absent and whose
/// kind is "sliding" is refused with the PRIMARY code `entity-id-collision`
/// — not "not a valid batch" — and the package is untouched.
#[test]
fn apply_refuses_a_raw_opening_with_its_primary_r4_4_code() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    let level = read_package(&path).expect("reads").snapshot.levels[0].id;
    let before = std::fs::read(&path).expect("readable");
    let mut opening = domowik_format::Opening::door(Uuid::now_v7(), 0.5, 900.0, 2100.0, 200.0);
    opening.id = level;
    let batch = Batch::new(
        Uuid::from_u128(0xCC),
        1,
        BTreeMap::new(),
        vec![Operation::create(Entity::Opening(opening))],
    );
    let mut raw = serde_json::to_value(&batch).expect("serialises");
    raw["operations"][0]["entity"]["kind"] = serde_json::json!("sliding");
    let batch_path = dir.path().join("batch.json");
    std::fs::write(&batch_path, serde_json::to_vec(&raw).expect("serialises")).expect("writes");

    let out = run(&path, "apply", &[batch_path.to_str().expect("utf-8")]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("command rejected: entity-id-collision"),
        "{stderr}"
    );
    assert_eq!(
        std::fs::read(&path).expect("readable"),
        before,
        "package untouched"
    );
}

/// R4.4's ranking applies when a raw batch would take EFFECT: a batch with
/// an invalid opening enum that would be HELD stays held (nothing saved),
/// and one with a framing error reports the framing error (Codex
/// GPT-6.1-Sol).
#[test]
fn apply_keeps_held_and_framing_outcomes_for_raw_openings() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = plan_on_disk(&dir);
    let before = std::fs::read(&path).expect("readable");
    let raw_batch = |frontier: BTreeMap<Uuid, u64>, counter: u64| {
        let opening = domowik_format::Opening::door(Uuid::now_v7(), 0.5, 900.0, 2100.0, 200.0);
        let batch = Batch::new(
            Uuid::from_u128(0xDD),
            counter,
            frontier,
            vec![Operation::create(Entity::Opening(opening))],
        );
        let mut raw = serde_json::to_value(&batch).expect("serialises");
        raw["operations"][0]["entity"]["kind"] = serde_json::json!("sliding");
        raw
    };
    let write = |name: &str, raw: &serde_json::Value| {
        let p = dir.path().join(name);
        std::fs::write(&p, serde_json::to_vec(raw).expect("serialises")).expect("writes");
        p
    };

    // A missing causal prerequisite: held, exit 0, package unchanged.
    let held = write(
        "held.json",
        &raw_batch(BTreeMap::from([(Uuid::from_u128(0xEE), 7)]), 1),
    );
    let out = run(&path, "apply", &[held.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Held"));
    assert_eq!(std::fs::read(&path).expect("readable"), before);

    // A stored batch redelivered with only its kind changed to an invalid
    // value is the E1 framing conflict, not an "already applied" duplicate.
    let host = read_package(&path)
        .expect("reads")
        .project()
        .expect("projects")
        .document
        .walls[0]
        .id;
    let mut valid = raw_batch(BTreeMap::new(), 1);
    valid["operations"][0]["entity"]["hostWallId"] = serde_json::json!(host);
    let mut changed = valid.clone();
    let mut valid_fixed = valid.clone();
    valid_fixed["operations"][0]["entity"]["kind"] = serde_json::json!("door");
    let stored = write("stored.json", &valid_fixed);
    accepted(&path, "apply", &[stored.to_str().unwrap()]);
    changed["operations"][0]["entity"]["kind"] = serde_json::json!("sliding");
    let redelivered = write("changed.json", &changed);
    let after_store = std::fs::read(&path).expect("readable");
    let out = run(&path, "apply", &[redelivered.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("different content"));
    assert_eq!(std::fs::read(&path).expect("readable"), after_store);
    let before = after_store;

    // A framing error (counterBase 0) is reported as such.
    let framing = write("framing.json", &raw_batch(BTreeMap::new(), 0));
    let out = run(&path, "apply", &[framing.to_str().unwrap()]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("invalid-opening-kind"),
        "framing first: {stderr}"
    );
    assert_eq!(std::fs::read(&path).expect("readable"), before);
}
