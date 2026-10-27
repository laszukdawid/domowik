/* Domowik bridge spike (issue 0024) — C ABI.
 *
 * SPIKE ONLY. Not a product contract; expected to change.
 *
 * Hand-written rather than generated. The surface is small enough that a
 * generator would be a build dependency and a licence review
 * (docs/THIRD-PARTY-POLICY.md) bought for about eighty lines, and a
 * hand-written header can carry the ownership rules where the caller reads
 * them. The Rust side is the authority; `charness/main.c` links against THIS
 * file, so a drift between the two fails the harness rather than passing
 * silently.
 *
 * OWNERSHIP, in one place:
 *
 *  - Every `domowik_buf` this library writes is owned by the CALLER and must
 *    be released with `domowik_buffer_free(buf.token)`. Nothing else may free
 *    it — the library's allocator is not libc's.
 *  - Release takes the buffer's opaque TOKEN, never its pointer. Tokens are
 *    monotonic and never reused, so `domowik_buffer_free` refuses an unknown
 *    or already-reclaimed one EXACTLY, returning DOMOWIK_UNKNOWN_BUFFER
 *    without freeing anything. A pointer-keyed registry could not promise
 *    that: every zero-length allocation shares one address, so two empty
 *    buffers collided, and a freed address may later be handed out again.
 *  - Request buffers are BORROWED read-only for the duration of the call. The
 *    caller must keep them valid and unmutated until the call returns, and may
 *    free them immediately after.
 *  - A call that returns non-zero does NOT write to its out-parameter and does
 *    NOT transfer a buffer.
 *  - Handles are opaque tokens, never pointers. A forged or closed handle is
 *    DETECTED (DOMOWIK_UNKNOWN_HANDLE), not dereferenced.
 *
 * THREADING: calls that touch a plan's SESSION serialise on that plan; distinct
 * handles proceed in parallel. `domowik_cancel` and `domowik_busy` deliberately
 * do NOT serialise against them — a cancel that waits for the work it cancels
 * is a wait — so "every call on one handle serialises" would be false and is
 * not claimed here. `domowik_cancel` may be called from any thread while an
 * operation on that handle is running; that is what it is for, and a recorded
 * cancellation is never erased by another operation starting.
 */

#ifndef DOMOWIK_BRIDGE_H
#define DOMOWIK_BRIDGE_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Status codes. Zero is success. */
#define DOMOWIK_OK 0
#define DOMOWIK_INVALID_ARGUMENT 1
#define DOMOWIK_UNKNOWN_HANDLE 2
#define DOMOWIK_UNKNOWN_BUFFER 3
#define DOMOWIK_MALFORMED_REQUEST 4
#define DOMOWIK_REJECTED 5
#define DOMOWIK_PANIC 6
#define DOMOWIK_CANCELED 7
#define DOMOWIK_EXHAUSTED 8

/* An owned byte buffer. `len` is exact; the bytes are NOT null-terminated.
 *
 * `token` is the release key — pass it, not `data`, to domowik_buffer_free.
 * Read the bytes with the LENGTH: strlen/strstr/toDartString on `data` read
 * past the end. See finding F1 in the spike README, which is exactly that
 * mistake, made by this project's own harness and caught by ASan. */
typedef struct {
  uint8_t *data;
  uint64_t len;
  uint64_t token;
} domowik_buf;

/* The ABI revision, currently 22. A caller reading a different number must
 * not proceed.
 *
 * 10 is NOT backward-compatible, unlike every export- or field-only bump
 * before it: domowik_plan_edit's SUCCESS response changed shape, from the
 * bare batch_id bytes it always wrote to the same JSON object
 * domowik_plan_execute/undo/redo return. A caller built against 9 must not
 * parse domowik_plan_edit's output; see its own comment below.
 *
 * 11 adds domowik_plan_marquee (issue 0059) and is additive like 9's bump,
 * not breaking like 10's: a caller that never looks the new export up is
 * unaffected. 12 likewise adds domowik_plan_snap and its debug build probe.
 *
 * 13 adds room records (issue 0060) and adds NO export: three new
 * domowik_plan_edit gesture kinds — addRoom, setRoom, deleteRoom — and the
 * recordId, label and displayAreaM2 fields plus a roomRecords array on
 * domowik_plan_derived. Eager symbol resolution still succeeds on 12, which
 * is exactly why the number has to move: a caller on 12 sending addRoom gets
 * a parse rejection, and one reading rooms[i] without label draws an unnamed
 * plan that looks like a plan with no room records in it. */
int32_t domowik_abi_version(void);

/* ABI 19: read-only preview of a descending stair drawing. Request:
 * {upperLevelId,startMm,endMm,name?,widthMm?,risers?,guardHeightMm?}.
 * start is on the current floor, end targets the nearest strictly lower floor.
 * Returns Rust-derived construction graphics with staircase inspector fields.
 * Same request is committed by edit {kind:"addDescendingStaircase",request:{...}}.
 * setStaircase additionally accepts name, startElevationMm (upper start) and
 * endElevationMm (lower end); absolute elevations become level-relative offsets.
 * Derived plan responses also carry constructionDiagnostics: strings naming
 * invalid stair assemblies omitted from the drawing. Other level geometry still
 * renders; invalid editing requests continue to reject atomically.
 * Calls are synchronous and serialized per plan; no history on preview, no
 * callbacks or retained pointers, standard panic/status and buffer ownership.
 * Available on native and browser hosts. Invalid requests leave out untouched. */
int32_t domowik_plan_stair_preview(uint64_t handle,const uint8_t *req,
                                 uint64_t req_len,domowik_buf *out);

/* ABI 18 adds architectural editing through domowik_plan_edit:
 * addStaircase {request:{name,from,to,widthMm,risers,guardHeightMm,cutUpperFloor}},
 * convertRoomToStaircase {roomId,lowerLevelId,upperEdge,goingMm,risers,guardHeightMm},
 * setStaircase {staircaseId,widthMm?,risers?,guardHeightMm?},
 * deleteStaircase {staircaseId}. Endpoints are {levelId,positionMm,offsetMm}.
 * Derived responses add floorSurfaces (cut floors, not room topology) and
 * construction (Rust-derived body/stair outlines, segments and UP/DOWN labels).
 * Requests are synchronous, serialized per plan, atomic and undoable. Existing
 * buffer ownership, panic/status transport and cancellation rules are unchanged;
 * no callbacks, retained client pointers or GPU resources are introduced. The
 * JSON contract is shared by the native and browser hosts. */

/* Creates an internal empty builder; add a level before saving. */
int32_t domowik_plan_create(uint64_t *out_handle);
/* Creates the editor baseline: one Level 1 at elevation 0, clean, no history.
 * Same ownership and threading rules as plan_create. */
int32_t domowik_plan_create_default(uint64_t *out_handle);

/* Executes a JSON `{"operations":[...]}` request as one atomic command.
 * Writes a JSON summary to `out`:
 *
 *   {"batch_id": <string>, "entities": <unsigned>, "undo_depth": <unsigned>,
 *    "redo_depth": <unsigned>, "dirty": <bool>,
 *    "warnings": [{"code": <string>, "level": <UUID>, "message": <string>,
 *                  "other"?: <UUID>, "below"?: <UUID>, "wall"?: <UUID>}]}
 *
 * `warnings` is the diagnostics this batch's FINAL state carries per
 * BEHAVIOR-SPEC 2.1 and R3.1.2's wall-area-overlap row — non-rejecting,
 * never a reason this call fails, computed fresh for the batch that just
 * committed (BEHAVIOR-SPEC R3.1.7: a warning attaches to the batch result
 * itself). domowik_plan_undo, domowik_plan_redo and (as of ABI 10)
 * domowik_plan_edit return this SAME shape; domowik_plan_state additionally
 * carries document_revision, described on it below. */
int32_t domowik_plan_execute(uint64_t handle, const uint8_t *req,
                             uint64_t req_len, domowik_buf *out);

/* ABI 15: plan-local model libraries and original 2D footprints (§10.1).
 * JSON kinds: list, preview, size, scene, mesh, attach, replaceDirectory, place,
 * set, delete, hit, copy, cut, paste. Request memory is borrowed;
 * success returns one buffer freed by its token. Calls serialize on the plan
 * lock and participate in close/drain. No callbacks or network access.
 * File import is synchronous and bounded by L5; callers may use a worker thread.
 * No import cancellation; a nonzero return transfers no buffer. */
/* preview/size read bounded embedded bytes, then decode outside the plan lock
 * while retaining its close/drain lease. It returns derived PNG bytes in JSON.
 * size instead returns dimensionsMm [width,depth,height], physicalUnits and
 * heightRatio from bounded static geometry decode. No source bytes are changed.
 * place optionally accepts a validated sourceDirectory from user config.
 * Without dimensionsMm it derives GLB metre dimensions or unitless proportions;
 * unreadable bounds use a labelled 1 m cube. Legacy explicit footprints remain
 * accepted. Size/placement decode is synchronous; import should use a worker.
 * set accepts optional heightMm and commits all supplied dimensions atomically.
 * pose takes instanceId, positionMm [plan-x,plan-y,elevation-offset],
 * rotationDeg [X,Y,Z] and optional unscaled heightMm. preview:true changes only
 * derived views, never save/undo state; clearPosePreview removes that overlay.
 * Without preview, pose commits one atomic placement edit. Document revisions
 * invalidate an outstanding preview; closing the plan releases it.
 * copy/cut take instanceIds and referenceId (the anchor, which must be among
 * them) and fill the plan-session clipboard; cut also commits the captured
 * deletions as ONE batch, and a refusal leaves document and clipboard
 * unchanged. paste takes levelId and positionMm and creates fresh-identity
 * copies of every captured instance on that level, rebased so the anchor's
 * origin lands at positionMm, as one atomic batch; it refuses whole when the
 * clipboard is empty, the level is unknown, or any captured digest's bytes are
 * no longer embedded. Copy and cut return the list described below; paste
 * returns it with the new anchor's "instanceId" merged in. The clipboard is
 * view state: never persisted, never in the operation log, not restored by
 * undo, and released with the plan handle. list additionally carries
 * "clipboardCount", ADDITIVE without an ABI revision, like document_revision.
 * Adding these JSON fields/kinds does not change the C function signature. */
/* thumbnail {digest} or {file,sourceDirectory?} returns the same PNG/triangle/
 * modelExtents shape as preview, using a fixed angled aspect-preserving camera.
 * File roots are canonicalized from the supplied selected library directory
 * and/or attached plan directories; out-of-root files are refused. Reads and
 * decode are bounded by the existing per-blob/model budgets and run outside
 * the session lock while retaining the close/drain lease. Read-only: no import,
 * directory attachment, dirty/history/revision or metadata changes, even on
 * failure. No cancellation, callbacks, network or external model resources.
 * Embedded thumbnails work on all core hosts; file requests require filesystem
 * access. Synchronous call; native UI callers use a worker and serialize work.
 * Ownership/errors/panic transport match other asset requests. */
/* ABI 20: materials {digest} returns [{key,name,color:[sRGB r,g,b,a]}].
 * colors {instanceId,materialColors:{key:"#RRGGBB"}} replaces one palette in
 * one undoable batch; {} resets, unchanged writes nothing. list includes each
 * instance's materialColors. preview additionally accepts materialColors.
 * Material queries/preview decode are read-only, bounded and outside the plan
 * lock; colors is serialized with other mutations. Ownership, close/drain,
 * panic transport, platform support and lack of cancellation are unchanged. */
int32_t domowik_plan_assets(uint64_t handle, const uint8_t *req,
                           uint64_t req_len, domowik_buf *out);

/* Undoes the most recent command, returning the same JSON shape as
 * domowik_plan_execute. `batch_id` is empty if there was none. */
int32_t domowik_plan_undo(uint64_t handle, domowik_buf *out);
/* Redo returns the same owned JSON summary as undo. Empty history is a no-op. */
int32_t domowik_plan_redo(uint64_t handle, domowik_buf *out);
/* Read-only live-session summary: the same batch_id (empty)/entities/
 * undo_depth/redo_depth/dirty/warnings shape as domowik_plan_execute, plus:
 *
 *   "document_revision": <unsigned>
 *
 * ADDITIVE over domowik_plan_execute's shape, and NOT an ABI revision change
 * on its own: this entry point's signature and every field it already wrote
 * are unchanged; document_revision is the only field it carries that the
 * others do not. Free out through domowik_buffer_free, as for execute.
 *
 * ABI 16 adds "asset_revision": <unsigned>, which grows when embedded asset
 * content changes without a batch (domowik_plan_import_model supplying the
 * bytes a placed instance lacked). Footprints, hit candidates and the 3D
 * scene can change with it while document_revision stays put.
 *
 * Warnings are non-rejecting diagnostics, ordered by the level rules in
 * BEHAVIOR-SPEC 2.1, then by R3.1.2's wall-area-overlap pairs. Computed once
 * per accepted revision and shared with whichever mutating call last ended
 * it — never persisted or undoable.
 *
 * A monotonic count of ACCEPTED DOCUMENT CHANGES on the active session — the
 * number of batches in its operation log. Every accepted execute, edit, undo
 * and redo advances it by one; an undo appends a COMPENSATING batch rather
 * than removing one, so it only ever grows. Rejected commands, every read,
 * and saving do not move it.
 *
 * Deliberately NOT the undo depth (which falls on undo and cannot tell
 * "undone" from "never done") and NOT the dirty flag (a boolean, cleared by
 * saving). A caller asking "did the document change at all since I last
 * looked" — a click-cycle reset, a cache invalidation — must read THIS, and
 * must not infer it from those two or from comparing entity IDs: an edit far
 * from the point of interest changes the document without changing any ID
 * near that point.
 *
 * It answers the DOCUMENT question only. The hit-test index is cached per
 * (level, revision), so querying a different level rebuilds it with this
 * number unchanged; a stable revision does not mean the index was reused. The
 * active level is a separate reset trigger the caller already tracks.
 *
 * Comparable only WITHIN one handle. The baseline belongs to the session, so
 * a new handle starts from its own: plan_create and plan_create_default begin
 * at 0, and plan_open begins at the file's retained batch count, because
 * opening CONTINUES the log rather than starting a new stream. */
int32_t domowik_plan_state(uint64_t handle, domowik_buf *out);

/* Writes the current document as JSON. */
int32_t domowik_plan_snapshot(uint64_t handle, domowik_buf *out);

/* Performs one EDITING GESTURE with every effect the specification attaches
 * to it, writing the SAME JSON shape as domowik_plan_execute to `out` —
 * batch_id, entities, undo_depth, redo_depth, dirty, warnings — including
 * any warning the resulting batch attaches (BEHAVIOR-SPEC R3.1.7). An
 * unchanged `setLevel` succeeds with an EMPTY owned buffer, not this shape:
 * there is no batch, so nothing to report, and history does not change.
 * `setLevel` accepts levelId and optional name/elevationMm/floorThicknessMm/
 * defaultWallHeightMm/defaultCeilingHeightMm. An omitted addWall heightMm
 * resolves the target level's current default once; explicit heights win.
 *
 * BREAKING as of ABI 10 (see domowik_abi_version above): every earlier
 * revision wrote only the bare batch_id bytes here, no JSON at all. A caller
 * built against ABI 9 must not parse this call's output — check
 * domowik_abi_version() before using it, not after a parse fails.
 *
 * NOT the same as domowik_plan_execute, which takes operations the caller
 * already decided on. A gesture is an intention whose consequences belong to
 * the specification: drawing a wall across another SPLITS it, the split lands
 * in the same batch, the openings on the crossed wall are remapped or the
 * whole edit is refused, and an endpoint landing on an occupied node joins it.
 * A caller assembling operations by hand gets none of that and quietly
 * produces a different document.
 *
 * ABI 17 adds setWalls {wallIds, thicknessMm?, heightMm?}: selected same-level
 * walls only, one atomic batch. Unknown/non-wall/cross-level IDs reject the whole
 * request. Duplicate IDs apply once in stable ID order; omitted or unchanged
 * fields emit no operations. An unchanged request returns an EMPTY owned buffer
 * without changing history. Hosted openings are revalidated on the final state;
 * a refusal changes neither document nor history. No length or axis edit.
 * Ownership, threading and errors are the same as other gestures.
 *
 * ABI 20 adds setWallColors {wallIds,color:"#RRGGBB"|null}, setting or resetting
 * distinct same-level walls atomically; unchanged colors produce no batch.
 * ABI 21 adds setDoorAppearance {openingId,color:"#RRGGBB"|null,
 * doorStyle:"solid"|"glass"}. All fields are required. Replaces a door leaf's
 * color/style in one atomic undoable edit; unchanged values produce no batch.
 * Windows/unknown IDs are refused. Derived openings include color, doorStyle,
 * leafColor (resolved technical ink, or null for legacy ink), originalColor.
 * Glass uses alpha .25 times level ghost alpha; frames/geometry are unchanged.
 * Ownership, allocation, close/drain, serialization/threading, panic transport,
 * cancellation (none) and native/browser support match other edit requests.
 * Requests are JSON: addWall, moveWall, setWall, setWalls, deleteWall, reverseWall,
 * addOpening, moveOpening, deleteOpening, deleteLevel, setLevel. */
int32_t domowik_plan_edit(uint64_t handle, const uint8_t *req, uint64_t req_len,
                          domowik_buf *out);

/* Opens the package at `path` and registers it as a new plan. Writes the
 * handle on success and nothing on failure. Opening CONTINUES the file's
 * operation log rather than starting a new stream. */
int32_t domowik_plan_open(const char *path, uint64_t *out_handle);

/* Writes the plan to `path`, adopting it as the plan's file. */
int32_t domowik_plan_save(uint64_t handle, const char *path);

/* ABI 16: the byte boundary for hosts without a filesystem (the browser
 * editor, issue 0088). Inputs are borrowed for the call; outputs are
 * token-owned buffers as elsewhere, and nothing is written on failure.
 *
 * open_bytes is domowik_plan_open over a package held in memory: the same
 * reader and refusals; the plan has no path.
 *
 * encode returns the bytes domowik_plan_save would write, and the plan's save
 * revision, without changing any state. mark_saved(revision) then clears
 * dirty, or refuses with DOMOWIK_REJECTED if the plan changed since that
 * encode, so an edit made while the host was storing the bytes stays unsaved.
 *
 * import_model embeds an uploaded GLB/OBJ/ZIP/USD model (UTF-8 file `name` or
 * source path, its `bytes`) in the plan's library and returns JSON
 * {digest, name}; paths are retained in asset provenance. Place it with
 * domowik_plan_assets {"kind":"place","digest":...}.
 *
 * scene_frame derives the visible levels' 3D scene, request JSON
 * {levelIds, ghostedIds?, poseOnly?, sceneToken?}, as one binary scene-wire
 * frame. With poseOnly:true and the last installed sceneToken, a matching
 * baseline returns extra.instanceOnly:true and names-only mesh entries: the
 * receiver MUST retain resident geometry, not upload empty mesh replacements.
 * Revision/visibility/token mismatch returns a full frame and fresh sceneToken.
 * Omitting poseOnly preserves the existing full-frame contract. */
int32_t domowik_plan_open_bytes(const uint8_t *bytes, uint64_t len,
                                uint64_t *out_handle);
int32_t domowik_plan_encode(uint64_t handle, uint64_t *out_revision,
                            domowik_buf *out);
int32_t domowik_plan_mark_saved(uint64_t handle, uint64_t revision);
int32_t domowik_plan_import_model(uint64_t handle, const uint8_t *name,
                                  uint64_t name_len, const uint8_t *bytes,
                                  uint64_t len, domowik_buf *out);
int32_t domowik_plan_scene_frame(uint64_t handle, const uint8_t *req,
                                 uint64_t req_len, domowik_buf *out);

/* ABI 22. Writes every level as one .usdz 3D scene (walls, openings, floors,
 * construction bodies, stairs and placed models with their colours) into
 * `out`, and into `out_diagnostics` a UTF-8 JSON array of warning strings
 * that name every model exported as a placeholder box ([] when none). Both
 * are token-owned buffers released together; free each. `out` and
 * `out_diagnostics` must be non-null, non-overlapping domowik_buf pointers
 * (INVALID_ARGUMENT otherwise). Read-only:
 * revision and dirty state are unchanged. Failure writes neither buffer and
 * records domowik_last_error. */
int32_t domowik_plan_export_usdz(uint64_t handle, domowik_buf *out,
                                 domowik_buf *out_diagnostics);

/* Writes one level's DERIVED geometry as JSON: the occupied region, the room
 * faces with their areas, and the level's floor area.
 *
 * The snapshot above returns wall centre lines; an editor drawing only those
 * would be drawing sticks. Rooms and the occupied region are COMPUTED and
 * never stored, and computing them in the UI would put a second geometry
 * implementation in the view.
 *
 * `level_id` is a NUL-terminated UTF-8 UUID. An unknown level yields empty
 * lists rather than an error: a level with no walls has no derived geometry. */
int32_t domowik_plan_derived(uint64_t handle, const char *level_id,
                             domowik_buf *out);

/* Ranked hit candidates at one point on one level, as JSON.
 *
 * Request, as `req_len` bytes at `req`:
 *
 *   {"levelId":"<uuid>","pointMm":[x,y],"radiusMm":120.0}
 *
 * `radiusMm` is the logical-pixel pick radius (12 mouse/stylus, 24 touch)
 * ALREADY converted to plan millimetres by the caller's view transform — the
 * core has none and this call does not invent one. It must be finite and in
 * [0, 240000]; 240000 is 24 px at the 0.0001 px/mm zoom floor, so a larger
 * value is an unclamped viewport rather than a gesture and is REFUSED.
 *
 * Response: an ORDERED array, top-ranked first, in the specification's class
 * precedence and total key.
 *
 *   [{"class":"join-node","entityId":"<uuid>","closestMm":[x,y],
 *     "distanceSquaredMm2":0.0}]
 *
 * `class` is join-node, free-endpoint, region or placeholder-box; the last is
 * defined for completeness and is NOT currently emitted, because this spike
 * passes no degraded instances to the hit path. `entityId` is the JOIN at a
 * node and the WALL at an endpoint or region. Distances are SQUARED and never
 * rooted — rooting makes two candidates that differ compare equal. They are
 * binary64, not exact reals: issue 0012 owns that gap.
 *
 * NO selection policy is applied. Click cycling, selection and the primary
 * item belong to the caller; answering them here would put one policy in two
 * places.
 *
 * Empty is not an error, and is distinct from a refusal:
 *  - a malformed request, a non-finite coordinate or an out-of-range radius
 *    returns DOMOWIK_MALFORMED_REQUEST and writes nothing;
 *  - a FINITE point beyond the plan domain widened by the radius returns an
 *    empty array without building or querying an index — panning a long way
 *    is ordinary, not an error;
 *  - an unknown level returns an empty array, as plan_derived does.
 *
 * ONE index is cached, reused while the query's level and document version
 * still match the index being held, so a drag pays for construction once
 * rather than per pointer move. Alternating between two levels rebuilds on
 * every call: that is what a single-entry cache costs, and only the active
 * level is hittable anyway. */
int32_t domowik_plan_hit_test(uint64_t handle, const uint8_t *req,
                              uint64_t req_len, domowik_buf *out);

/* Ranked section-6 snapping candidates. Request JSON is:
 * {"levelId":<uuid>,"pointMm":[x,y],"radiusMm":r,
 *  "visibleRectMm":[minX,minY,maxX,maxY],"anchorMm"?:[x,y],
 *  "previousDirection"?:[dx,dy],"excludedWallIds"?:[uuid],
 *  "excludedJoinIds"?:[uuid],"gridSpacingMm"?:number,
 *  "gridEnabled"?:bool,"objectEnabled"?:bool,"angleEnabled"?:bool,
 *  "altDisabled"?:bool}.
 *
 * radiusMm has the same finite [0,240000] bound as hit testing. The visible
 * rectangle and optional coordinates/direction must be finite and ordered.
 * Responses are {"candidates":[{"class", "entityId", "pointMm",
 * "feature", "guideMm"}]}; entityId and guideMm may be JSON null. Feature
 * has kind entity|edge|axis|grid|angle plus the core's feature fields.
 * Unknown levels have no document sources but local grid/angle remains valid.
 * Core invalid-input/grid-range/edge-ownership refusals return
 * DOMOWIK_MALFORMED_REQUEST and leave out untouched. */
int32_t domowik_plan_snap(uint64_t handle, const uint8_t *req,
                           uint64_t req_len, domowik_buf *out);

/* The R7.3 marquee query, READ-ONLY: entity IDs whose interaction shape
 * intersects the given rectangle, as JSON, in ascending marquee-key order.
 *
 * Request, as `req_len` bytes at `req`:
 *
 *   {"levelId":"<uuid>","corner1Mm":[x,y],"corner2Mm":[x,y]}
 *
 * The two corners may be given in EITHER order. Both must be finite: a
 * non-finite component returns DOMOWIK_MALFORMED_REQUEST and writes nothing
 * (section 1.4's "non-finite input discards the whole event", and R7.3.2).
 *
 * Response:
 *
 *   {"entityIds": ["<uuid>", ...]}
 *
 * This is the SAME order R7.3.2 requires additive marquee to add candidates
 * in, so a caller finds the new PRIMARY by taking the last of these not
 * already in its selection, and changes nothing when every one of them
 * already is. Only ACTIVE-level entities are eligible (R7.1.3, R7.5.1,
 * R7.5.4) — the core's own level filter, not a second one here. A rectangle
 * with coincident corners degenerates to POINT-HIT semantics
 * at its corner (R7.3.2a), selecting at most the one top-ranked candidate
 * there, exactly as domowik_plan_hit_test's own class precedence would.
 *
 * INHERITED, NOT FIXED, by this transport: issue 0012's own open gaps.
 * Distances and the occupied-polygon/closest-point construction the core's
 * exact-boundary test runs against are binary64, not exact-real, arithmetic
 * (the same caveat domowik_plan_hit_test's own doc states above).
 * Degraded instances participate through per-instance local resolution:
 * declared width/depth are scaled/rotated before taking axis-aligned bounds;
 * absent dimensions use the fixed 1000 mm square ignoring scale/rotation.
 * Resolved instances also participate (issue 0111), using the axis-aligned
 * envelope of their displayed footprint, including tilt projection. Their
 * candidates share the positional marquee key with walls/placeholders; on a
 * min-corner tie the resolved-asset class ranks last. Degraded instances are
 * included once as placeholders, never also as resolved assets.
 * Required degradation permits inspection but refuses mutations/saving.
 *
 * READ-ONLY: never commits an operation, never advances document_revision,
 * never touches undo/redo. Selection is local view state, never document
 * history — this call is how that state gets COMPUTED, not a mutation. Queries
 * reuse the revision-keyed geometry and footprint indexes, and the marquee
 * merges their candidates. It is expected to be called ONCE per gesture
 * (typically on release), not per pointer move. */
int32_t domowik_plan_marquee(uint64_t handle, const uint8_t *req,
                             uint64_t req_len, domowik_buf *out);

/* The handle's sticky last error as UTF-8. Retrieval does NOT clear it. */
int32_t domowik_last_error(uint64_t handle, domowik_buf *out);

/* Releases a buffer this library issued, by its token. Any value is safe to
 * pass: an unknown token is refused, never acted on. See OWNERSHIP above. */
int32_t domowik_buffer_free(uint64_t token);

/* Closes a plan. The handle is never reissued.
 *
 * Returns only after every call already admitted on this plan — one that has
 * taken its lease, whether or not it had reached the plan lock yet — has
 * finished its lease-bracketed work, and every effect it publishes THROUGH
 * THIS LIBRARY is visible: completed session mutations, a written *out
 * buffer with its token registered, and any last-error update. The call's
 * own status CODE is delivered to its own caller only after the lease drops,
 * so a call whose only output is a status code (domowik_busy,
 * domowik_cancel) publishes nothing this promise covers; close guarantees
 * its body ran to completion, not that its caller has the result in hand. A
 * call already admitted still completes and returns its normal status, and
 * its effects go away with the plan: closing is the caller's statement that
 * it no longer wants the result.
 *
 * This is NOT permission to unload the library. A calling thread can still be
 * a few instructions past its lease drop, unwinding that call's own epilogue,
 * when domowik_plan_close returns on another thread. A caller that wants to
 * dlclose/FreeLibrary this library must still join every thread of its own
 * that could be inside an ABI call, and free every buffer it still owns,
 * before doing so — this call closes a PLAN, not the library. */
int32_t domowik_plan_close(uint64_t handle);

/* Panics on purpose, to prove containment from this side of the boundary. */
int32_t domowik_debug_panic(void);

/* Spins for up to `millis`, observing the handle's cancellation flag. */
int32_t domowik_busy(uint64_t handle, uint64_t millis);

/* Requests cancellation. Safe to call while `domowik_busy` is running. */
int32_t domowik_cancel(uint64_t handle);

/* DEBUG: the handle's cancellation epoch, or -1 if unknown. Exposed so the
 * invariant "a recorded cancellation is never erased" is testable. */
int64_t domowik_debug_cancel_epoch(uint64_t handle);

/* DEBUG: panics while holding the plan lock, to exercise poison recovery. */
int32_t domowik_debug_panic_holding_lock(uint64_t handle);

/* DEBUG: how many times this handle has BUILT a hit index, or -1 if the handle
 * is unknown. Exposed because a cache that rebuilt on every query would return
 * byte-identical answers, so a test written against the RESULTS passes on the
 * broken implementation. This makes "reused across reads and saves, dropped by
 * an edit" directly assertable. */
int64_t domowik_debug_hit_index_builds(uint64_t handle);
/* Number of built document-level SnapIndex values, or -1 for unknown handle. */
int64_t domowik_debug_snap_index_builds(uint64_t handle);

/* DEBUG: how many times this handle's warning cache has actually RECOMPUTED
 * (a revision miss), or -1 if the handle is unknown. A mutating call and the
 * domowik_plan_state read that immediately follows it share ONE computation
 * per accepted revision; both would return byte-identical `warnings` whether
 * or not the second call recomputed, so that property is not otherwise
 * assertable from this ABI. */
int64_t domowik_debug_warnings_computations(uint64_t handle);

/* Buffers issued and not yet reclaimed. Zero after a well-behaved run. */
int64_t domowik_outstanding_buffers(void);

/* Plans currently open. */
int64_t domowik_open_plans(void);

#if defined(__wasi__)
/* Browser build only (wasm32-wasip1, issue 0088). The host cannot allocate in
 * the module's memory, so it borrows 8-byte-aligned blocks for requests and
 * out-parameters and returns each with the same length. These blocks are not
 * domowik_buf buffers and never go to domowik_buffer_free. */
uint8_t *domowik_web_alloc(uint32_t len);
void domowik_web_free(uint8_t *ptr, uint32_t len);
#endif

#if defined(__APPLE__)
/* macOS 3D renderer (issue 0101). Called only by the presenter compiled into
 * the same framework (frontend/plugin/macos/Classes), so these exports are
 * outside the Dart-facing ABI that domowik_abi_version() versions.
 *
 * Renderer handles and target ids are opaque and never reused; forged or
 * destroyed ones return DOMOWIK_UNKNOWN_HANDLE. The Metal device is borrowed
 * for the renderer's lifetime. attach_target CONSUMES one +1 retain of an
 * id<MTLTexture> (BGRA8Unorm, 2D, render-target usage, created on that
 * device), even on failure; detach/destroy release it.
 *
 * Calls on one renderer serialize. render blocks until the GPU finishes the
 * frame: call it from a render queue, never the main or raster thread.
 * load_scene derives the plan's scene without holding the renderer, holds the
 * plan's close lease throughout, and refuses a generation not greater than
 * every one already applied. last_error(0, ..) explains a failed create. */
int32_t domowik_r3d_create(uint64_t *out_handle);
int32_t domowik_r3d_destroy(uint64_t handle);
void *domowik_r3d_metal_device(uint64_t handle);
int32_t domowik_r3d_attach_target(uint64_t handle, void *texture,
                                  uint32_t width, uint32_t height,
                                  uint64_t *out_target);
int32_t domowik_r3d_detach_target(uint64_t handle, uint64_t target);
/* camera: 16 column-major floats; gl_depth nonzero = GL depth [-1, 1]. */
int32_t domowik_r3d_render(uint64_t handle, uint64_t target,
                           const float *camera, int32_t gl_depth);
/* req: JSON {levelIds, ghostedIds?}. Success writes JSON {generation,
 * originMm, center, radius, diagnostics, modelDecodes}. */
int32_t domowik_r3d_load_scene(uint64_t handle, uint64_t plan,
                               const uint8_t *req, uint64_t req_len,
                               uint64_t generation, domowik_buf *out);
int32_t domowik_r3d_orientation_probe(uint64_t handle);
int32_t domowik_r3d_request_diagnostic(uint64_t handle);
int32_t domowik_r3d_stats(uint64_t handle, domowik_buf *out);
int32_t domowik_r3d_last_error(uint64_t handle, domowik_buf *out);
#endif

#ifdef __cplusplus
}
#endif

#endif /* DOMOWIK_BRIDGE_H */
