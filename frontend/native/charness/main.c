/* Plain-C harness for the Domowik bridge spike (issue 0024).
 *
 * Drives the REAL shared library through dlopen, the way Dart's
 * `DynamicLibrary.open` does — not by linking the Rust crate, which would test
 * a different thing (the rlib, with Rust's own type checking still in play).
 *
 * What is proven here that the Rust tests cannot prove:
 *
 *  - the exported symbols exist with the names and signatures the header
 *    declares, in the artefact that ships;
 *  - a panic is contained when it unwinds into a FOREIGN frame, which is the
 *    case the language rules are actually about;
 *  - ownership abuse from a caller with no borrow checker: forged handles,
 *    stale handles, double free, foreign pointers;
 *  - several OS threads, none of them created by Rust, calling in at once.
 *
 * Built with -fsanitize=address,undefined, so the run is itself a
 * memory-safety gate rather than only a functional one. ASan LEAK detection is
 * unavailable on macOS/arm64 ("detect_leaks is not supported on this
 * platform"), so leaks are checked instead by the bridge's own
 * `domowik_outstanding_buffers` and `domowik_open_plans` counters at the end
 * of the run. That is a weaker check than LeakSanitizer and is recorded as
 * such in the README rather than glossed.
 *
 * Exit 0 on success; any failure prints and exits 1.
 */

#define _GNU_SOURCE
#include <dlfcn.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "../include/domowik_bridge.h"

static int checks = 0;
static int failures = 0;

static void check(int ok, const char *what) {
  checks++;
  if (!ok) {
    failures++;
    fprintf(stderr, "FAIL: %s\n", what);
  }
}

/* A NUL-terminated copy of a bridge buffer.
 *
 * `domowik_buf` is a LENGTH-DELIMITED byte range and is deliberately not
 * NUL-terminated: the payload is JSON, which may legitimately contain a zero
 * byte inside a string escape, and terminating it would make the length
 * redundant in the common case and wrong in the uncommon one.
 *
 * The first draft of this harness called `strstr` straight on `buf.data`.
 * AddressSanitizer caught the heap-buffer-overflow on the first run. Recorded
 * as finding F1 in the README: the header states the rule, and a C caller
 * broke it inside an hour anyway. Every string use goes through here.
 *
 * Caller frees with `free`. This memory is the harness's, never the bridge's.
 */
static char *as_cstring(const domowik_buf *buf) {
  char *out = (char *)malloc((size_t)buf->len + 1);
  if (!out) {
    fprintf(stderr, "FAIL: out of memory\n");
    exit(1);
  }
  memcpy(out, buf->data, (size_t)buf->len);
  out[buf->len] = '\0';
  return out;
}

static void check_contains(const domowik_buf *buf, const char *needle,
                           const char *what) {
  char *text = as_cstring(buf);
  check(strstr(text, needle) != NULL, what);
  free(text);
}

static void check_eq(int32_t got, int32_t want, const char *what) {
  checks++;
  if (got != want) {
    failures++;
    fprintf(stderr, "FAIL: %s (got %d, want %d)\n", what, got, want);
  }
}

/* Resolved symbols. */
static int32_t (*abi_version)(void);
static int32_t (*plan_create)(uint64_t *);
static int32_t (*plan_create_default)(uint64_t *);
static int32_t (*plan_execute)(uint64_t, const uint8_t *, uint64_t,
                               domowik_buf *);
static int32_t (*plan_undo)(uint64_t, domowik_buf *);
static int32_t (*plan_redo)(uint64_t, domowik_buf *);
static int32_t (*plan_state)(uint64_t, domowik_buf *);
static int32_t (*plan_snapshot)(uint64_t, domowik_buf *);
static int32_t (*plan_hit_test)(uint64_t, const uint8_t *, uint64_t,
                                domowik_buf *);
static int32_t (*plan_snap)(uint64_t, const uint8_t *, uint64_t, domowik_buf *);
static int32_t (*plan_marquee)(uint64_t, const uint8_t *, uint64_t,
                               domowik_buf *);
static int64_t (*debug_hit_index_builds)(uint64_t);
static int64_t (*debug_snap_index_builds)(uint64_t);
static int32_t (*last_error)(uint64_t, domowik_buf *);
static int32_t (*buffer_free)(uint64_t);
static int32_t (*plan_close)(uint64_t);
static int32_t (*debug_panic)(void);
static int32_t (*busy)(uint64_t, uint64_t);
static int32_t (*cancel)(uint64_t);
static int64_t (*outstanding_buffers)(void);
static int64_t (*open_plans)(void);

#define RESOLVE(sym, name)                                                     \
  do {                                                                         \
    *(void **)(&sym) = dlsym(lib, name);                                       \
    if (!sym) {                                                                \
      fprintf(stderr, "FAIL: symbol %s missing: %s\n", name, dlerror());       \
      return 1;                                                                \
    }                                                                          \
  } while (0)

/* A request creating one level. The uuid is fixed per call site; the bridge
 * does not care whether ids repeat across DISTINCT plans. */
static void level_request(char *out, size_t n, const char *uuid) {
  snprintf(out, n,
           "{\"operations\":[{\"operationId\":\"%s\",\"entityId\":\"%s\","
           "\"kind\":\"createEntity\",\"entity\":{\"type\":\"level\","
           "\"id\":\"%s\",\"name\":\"Ground\",\"elevationMm\":0.0,"
           "\"floorThicknessMm\":200.0,\"defaultWallHeightMm\":2400.0,"
           "\"defaultCeilingHeightMm\":2400.0}}]}",
           uuid, uuid, uuid);
}

/* A request creating one wall on `level`, so the hit-test probes below have
 * geometry to find. Coordinates are plan millimetres. */
static void wall_request(char *out, size_t n, const char *uuid,
                         const char *level, double x0, double y0, double x1,
                         double y1) {
  snprintf(out, n,
           "{\"operations\":[{\"operationId\":\"%s\",\"entityId\":\"%s\","
           "\"kind\":\"createEntity\",\"entity\":{\"type\":\"wall\","
           "\"id\":\"%s\",\"levelId\":\"%s\",\"startMm\":[%.1f,%.1f],"
           "\"endMm\":[%.1f,%.1f],\"thicknessMm\":200.0,"
           "\"heightMm\":2500.0}}]}",
           uuid, uuid, uuid, level, x0, y0, x1, y1);
}

/* A hit-test request. `radius` is already in plan millimetres.
 *
 * `%.17g`, not `%g`: the default six significant digits round 240000.1 to
 * "240000", which would turn the just-past-the-limit probe below into a
 * just-inside-the-limit one and quietly assert nothing. */
static void hit_request(char *out, size_t n, const char *level, double x,
                        double y, double radius) {
  snprintf(out, n,
           "{\"levelId\":\"%s\",\"pointMm\":[%.17g,%.17g],\"radiusMm\":%.17g}",
           level, x, y, radius);
}

static void marquee_request(char *out, size_t n, const char *level, double x0,
                            double y0, double x1, double y1) {
  snprintf(out, n,
           "{\"levelId\":\"%s\",\"corner1Mm\":[%.17g,%.17g],"
           "\"corner2Mm\":[%.17g,%.17g]}",
           level, x0, y0, x1, y1);
}

static void uuid_like(char *out, size_t n, unsigned seed) {
  snprintf(out, n, "0194%04x-6b18-7d3a-a153-1ee2f33cc%03x", seed & 0xffff,
           seed & 0xfff);
}

struct cancel_args {
  uint64_t handle;
  int32_t result;
};

/* Runs a long operation so the main thread can cancel it mid-flight. */
static void *cancel_worker(void *raw) {
  struct cancel_args *a = (struct cancel_args *)raw;
  a->result = busy(a->handle, 10000);
  return NULL;
}

struct worker_args {
  uint64_t handle;
  unsigned seed;
  int32_t result;
};

static void *worker(void *raw) {
  struct worker_args *a = (struct worker_args *)raw;
  char uuid[64];
  char req[512];
  uuid_like(uuid, sizeof uuid, a->seed);
  level_request(req, sizeof req, uuid);

  domowik_buf out = {NULL, 0, 0};
  a->result = plan_execute(a->handle, (const uint8_t *)req, strlen(req), &out);
  if (a->result == DOMOWIK_OK) {
    if (buffer_free(out.token) != DOMOWIK_OK) {
      a->result = -1;
    }
  }
  return NULL;
}

int main(int argc, char **argv) {
  if (argc < 2) {
    fprintf(stderr, "usage: %s <path-to-shared-library>\n", argv[0]);
    return 2;
  }
  /* Snyk flags this as CWE-94 (code injection): an unsanitised command-line
   * argument reaching dlopen. The finding is correct about the mechanism and
   * is ACCEPTED, not fixed, because loading a caller-named library is what
   * this program is for — the same trust model as `ldd` or the dynamic loader
   * itself. There is no sanitisation that preserves the purpose: any check
   * that narrowed the path would either reject the artefact under test or be
   * bypassable by the operator who already chose to run this.
   *
   * What bounds it: this harness is a test tool, never installed and never
   * distributed (the spike ships nothing), and the only party who can supply
   * argv is the party already running arbitrary code. Recorded in the README.
   */
  void *lib = dlopen(argv[1], RTLD_NOW);
  if (!lib) {
    fprintf(stderr, "FAIL: dlopen(%s): %s\n", argv[1], dlerror());
    return 1;
  }

  RESOLVE(abi_version, "domowik_abi_version");
  RESOLVE(plan_create, "domowik_plan_create");
  RESOLVE(plan_create_default, "domowik_plan_create_default");
  RESOLVE(plan_execute, "domowik_plan_execute");
  RESOLVE(plan_undo, "domowik_plan_undo");
  RESOLVE(plan_redo, "domowik_plan_redo");
  RESOLVE(plan_state, "domowik_plan_state");
  RESOLVE(plan_snapshot, "domowik_plan_snapshot");
  RESOLVE(plan_hit_test, "domowik_plan_hit_test");
  RESOLVE(plan_snap, "domowik_plan_snap");
  RESOLVE(plan_marquee, "domowik_plan_marquee");
  RESOLVE(debug_hit_index_builds, "domowik_debug_hit_index_builds");
  RESOLVE(debug_snap_index_builds, "domowik_debug_snap_index_builds");
  RESOLVE(last_error, "domowik_last_error");
  RESOLVE(buffer_free, "domowik_buffer_free");
  RESOLVE(plan_close, "domowik_plan_close");
  RESOLVE(debug_panic, "domowik_debug_panic");
  RESOLVE(busy, "domowik_busy");
  RESOLVE(cancel, "domowik_cancel");
  RESOLVE(outstanding_buffers, "domowik_outstanding_buffers");
  RESOLVE(open_plans, "domowik_open_plans");

  check_eq(abi_version(), 20, "abi version");

  /* ---- a stateful round trip ---- */
  uint64_t h = 0;
  check_eq(plan_create_default(&h), DOMOWIK_OK, "clean editor baseline");
  check(h != 0, "handle is non-zero");

  /* Grid is local: this has candidates before any document geometry exists. */
  {
    const char *snap_req =
        "{\"levelId\":\"00000000-0000-0000-0000-000000000000\","
        "\"pointMm\":[1,1],\"radiusMm\":10,"
        "\"visibleRectMm\":[-20,-20,20,20],\"gridSpacingMm\":10,"
        "\"objectEnabled\":false,\"angleEnabled\":false}";
    domowik_buf snap_out = {(uint8_t *)"sentinel", 8, 77};
    check_eq(plan_snap(h, (const uint8_t *)snap_req, strlen(snap_req), &snap_out),
             DOMOWIK_OK, "snap grid query");
    check_contains(&snap_out, "\"class\":\"grid\"", "snap returns grid");
    check_eq(buffer_free(snap_out.token), DOMOWIK_OK, "free snap response");
    check(debug_snap_index_builds(h) == 1, "snap index build counted");
  }

  char uuid[64], req[512];
  uuid_like(uuid, sizeof uuid, 1);
  level_request(req, sizeof req, uuid);
  domowik_buf out = {NULL, 0, 0};
  check_eq(plan_execute(h, (const uint8_t *)req, strlen(req), &out),
           DOMOWIK_OK, "execute");
  check(out.data != NULL && out.len > 0, "execute returned a buffer");
  check_contains(&out, "\"entities\":2", "execute added a level above baseline");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the response");

  /* State must persist across calls, or the handle means nothing. */
  out.data = NULL;
  check_eq(plan_snapshot(h, &out), DOMOWIK_OK, "snapshot");
  check_contains(&out, "Ground", "snapshot sees the level");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the snapshot");

  out.data = NULL;
  check_eq(plan_undo(h, &out), DOMOWIK_OK, "undo");
  check_contains(&out, "\"entities\":1", "undo preserves baseline");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the undo response");
  check_eq(plan_redo(h, &out), DOMOWIK_OK, "redo");
  check_contains(&out, "\"entities\":2", "redo restores level");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free redo");
  check_eq(plan_state(h, &out), DOMOWIK_OK, "state");
  check_contains(&out, "\"dirty\":true", "live session dirty");
  check_contains(&out, "\"document_revision\":3",
                 "execute, undo and redo each advance revision");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free state");
  uint64_t baseline = 0;
  check_eq(plan_create_default(&baseline), DOMOWIK_OK, "default plan");
  check_eq(plan_state(baseline, &out), DOMOWIK_OK, "baseline state");
  check_contains(&out, "\"entities\":1", "baseline ground level");
  check_contains(&out, "\"undo_depth\":0", "baseline not undoable");
  check_contains(&out, "\"dirty\":false", "baseline clean");
  check_contains(&out, "\"document_revision\":0", "baseline revision zero");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free baseline state");
  check_eq(plan_close(baseline), DOMOWIK_OK, "close baseline");

  /* ---- hit testing ---- */
  /* On a SCOPED plan of its own, created and closed here.
   *
   * The malformed probes below record a sticky last_error, and the
   * two-empty-buffer regression further down needs a handle whose error is
   * still EMPTY to build its fixture. Running these on `h` broke that fixture
   * — "the fixture needs empty buffers" — and the ASan run caught it. The
   * ownership probe is the older and more valuable of the two, so the new
   * block moves rather than the old one. Both sticky-error semantics and the
   * empty-buffer token regression stay exactly as they were. */
  {
    uint64_t hh = 0;
    char hit_level[64], hit_level_req[512];
    char wall_uuid[64], wall_req[640], hit_req[256];
    check_eq(plan_create(&hh), DOMOWIK_OK, "hit-test plan");
    uuid_like(hit_level, sizeof hit_level, 600);
    level_request(hit_level_req, sizeof hit_level_req, hit_level);
    out.data = NULL;
    check_eq(plan_execute(hh, (const uint8_t *)hit_level_req,
                          strlen(hit_level_req), &out),
             DOMOWIK_OK, "hit-test level");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the level response");

    uuid_like(wall_uuid, sizeof wall_uuid, 700);
    wall_request(wall_req, sizeof wall_req, wall_uuid, hit_level, 0.0, 0.0,
                 4000.0, 0.0);
    out.data = NULL;
    check_eq(plan_execute(hh, (const uint8_t *)wall_req, strlen(wall_req),
                          &out),
             DOMOWIK_OK, "create a wall to hit");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the wall response");

    /* On the wall's centre line, so its region is a candidate at exactly
     * zero: the endpoints are 2000 mm away and outside the 120 mm radius. */
    hit_request(hit_req, sizeof hit_req, hit_level, 2000.0, 0.0, 120.0);
    out.data = NULL;
    check_eq(plan_hit_test(hh, (const uint8_t *)hit_req, strlen(hit_req), &out),
             DOMOWIK_OK, "hit test");
    check_contains(&out, "\"class\":\"region\"", "the wall region is a hit");
    check_contains(&out, "\"distanceSquaredMm2\":0.0",
                   "a point on the centre line is at squared distance zero");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the hit response");

    /* Built once, reused after. A cache that rebuilt every time would return
     * byte-identical answers, so only the counter can see the difference. */
    int64_t builds = debug_hit_index_builds(hh);
    check(builds == 1, "the index was built exactly once");
    out.data = NULL;
    check_eq(plan_hit_test(hh, (const uint8_t *)hit_req, strlen(hit_req), &out),
             DOMOWIK_OK, "hit test again");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the second response");
    check(debug_hit_index_builds(hh) == builds, "the index was REUSED");

    /* Far outside the plan domain: answered with nothing, and no index work
     * is done to answer it. */
    hit_request(hit_req, sizeof hit_req, hit_level, 1e9, 0.0, 120.0);
    out.data = NULL;
    check_eq(plan_hit_test(hh, (const uint8_t *)hit_req, strlen(hit_req), &out),
             DOMOWIK_OK, "an out-of-domain point is answered, not refused");
    check(out.len == 2 && out.data[0] == '[' && out.data[1] == ']',
          "the answer is the empty array");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the empty response");
    check(debug_hit_index_builds(hh) == builds,
          "an out-of-domain query did no index work");

    /* The inclusive radius limit: 24 logical px at the 0.0001 px/mm floor. */
    hit_request(hit_req, sizeof hit_req, hit_level, 0.0, 0.0, 240000.0);
    out.data = NULL;
    check_eq(plan_hit_test(hh, (const uint8_t *)hit_req, strlen(hit_req), &out),
             DOMOWIK_OK, "the inclusive radius limit is accepted");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the wide response");

    /* Refusals write NOTHING. Checked against a sentinel rather than zeroes:
     * an implementation that zeroed the whole struct would satisfy a
     * `data == NULL` check while being exactly the bug it exists to catch. */
    char over[256], negative[256], overflowing[256];
    hit_request(over, sizeof over, hit_level, 0.0, 0.0, 240000.1);
    hit_request(negative, sizeof negative, hit_level, 0.0, 0.0, -1.0);
    snprintf(overflowing, sizeof overflowing,
             "{\"levelId\":\"%s\",\"pointMm\":[1e400,0],\"radiusMm\":120}",
             hit_level);
    const char *bad[] = {"{not json", over, negative, overflowing};
    const char *bad_what[] = {"malformed json",
                              "a radius past the viewport limit",
                              "a negative radius",
                              "a coordinate that overflows binary64"};
    for (size_t i = 0; i < sizeof bad / sizeof bad[0]; i++) {
      domowik_buf untouched;
      untouched.data = (uint8_t *)(uintptr_t)0xA5A5A5A5u;
      untouched.len = 0x5A5A5A5A;
      untouched.token = 0x1234;
      check_eq(plan_hit_test(hh, (const uint8_t *)bad[i], strlen(bad[i]),
                             &untouched),
               DOMOWIK_MALFORMED_REQUEST, bad_what[i]);
      check(untouched.data == (uint8_t *)(uintptr_t)0xA5A5A5A5u &&
                untouched.len == 0x5A5A5A5A && untouched.token == 0x1234,
            "a refused hit test leaves the out-parameter UNTOUCHED");
    }

    domowik_buf forged_sink;
    forged_sink.data = (uint8_t *)(uintptr_t)0xA5A5A5A5u;
    forged_sink.len = 0x5A5A5A5A;
    forged_sink.token = 0x1234;
    check_eq(plan_hit_test(0xdeadbeef, (const uint8_t *)hit_req,
                           strlen(hit_req), &forged_sink),
             DOMOWIK_UNKNOWN_HANDLE, "a forged handle is detected");
    check(forged_sink.data == (uint8_t *)(uintptr_t)0xA5A5A5A5u,
          "a forged hit test writes nothing");
    check(debug_hit_index_builds(0xdeadbeef) == -1,
          "the debug probe reports an unknown handle as -1");

    /* domowik_plan_marquee (issue 0059): a rectangle containing the wall
     * selects it, boundary-inclusive (R7.3.1), on the SAME handle/wall. */
    char mreq[256];
    marquee_request(mreq, sizeof mreq, hit_level, -100.0, -100.0, 4100.0,
                    100.0);
    out.data = NULL;
    check_eq(plan_marquee(hh, (const uint8_t *)mreq, strlen(mreq), &out),
             DOMOWIK_OK, "marquee containing the wall");
    check_contains(&out, wall_uuid, "the wall is selected");
    check_eq(buffer_free(out.token), DOMOWIK_OK, "free the marquee response");

    /* Reversed corners answer identically (R7.3.2: either order). */
    char mreq_rev[256];
    marquee_request(mreq_rev, sizeof mreq_rev, hit_level, 4100.0, 100.0,
                    -100.0, -100.0);
    out.data = NULL;
    check_eq(plan_marquee(hh, (const uint8_t *)mreq_rev, strlen(mreq_rev),
                          &out),
             DOMOWIK_OK, "reversed-corner marquee");
    check_contains(&out, wall_uuid, "reversed corners still select the wall");
    check_eq(buffer_free(out.token), DOMOWIK_OK,
             "free the reversed-corner response");

    /* A rectangle that misses entirely is answered, not refused. */
    marquee_request(mreq, sizeof mreq, hit_level, 10000.0, 10000.0, 11000.0,
                    11000.0);
    out.data = NULL;
    check_eq(plan_marquee(hh, (const uint8_t *)mreq, strlen(mreq), &out),
             DOMOWIK_OK, "a missing marquee is answered, not refused");
    check(out.len == strlen("{\"entityIds\":[]}") &&
              memcmp(out.data, "{\"entityIds\":[]}", out.len) == 0,
          "the answer is an empty entityIds array");
    check_eq(buffer_free(out.token), DOMOWIK_OK,
             "free the empty marquee response");

    /* A corner coordinate overflowing binary64 refuses the WHOLE call and
     * writes nothing, the same discipline plan_hit_test's own radius/point
     * refusals use above. */
    char moverflow[256];
    snprintf(moverflow, sizeof moverflow,
             "{\"levelId\":\"%s\",\"corner1Mm\":[1e400,0],"
             "\"corner2Mm\":[0,0]}",
             hit_level);
    domowik_buf muntouched;
    muntouched.data = (uint8_t *)(uintptr_t)0xA5A5A5A5u;
    muntouched.len = 0x5A5A5A5A;
    muntouched.token = 0x1234;
    check_eq(plan_marquee(hh, (const uint8_t *)moverflow, strlen(moverflow),
                          &muntouched),
             DOMOWIK_MALFORMED_REQUEST,
             "a corner coordinate overflowing binary64 is refused");
    check(muntouched.data == (uint8_t *)(uintptr_t)0xA5A5A5A5u &&
              muntouched.len == 0x5A5A5A5A && muntouched.token == 0x1234,
          "a refused marquee leaves the out-parameter UNTOUCHED");

    domowik_buf forged_marquee_sink;
    forged_marquee_sink.data = (uint8_t *)(uintptr_t)0xA5A5A5A5u;
    forged_marquee_sink.len = 0x5A5A5A5A;
    forged_marquee_sink.token = 0x1234;
    check_eq(plan_marquee(0xdeadbeef, (const uint8_t *)mreq, strlen(mreq),
                          &forged_marquee_sink),
             DOMOWIK_UNKNOWN_HANDLE, "a forged handle is detected (marquee)");
    check(forged_marquee_sink.data == (uint8_t *)(uintptr_t)0xA5A5A5A5u,
          "a forged marquee writes nothing");

    check_eq(plan_close(hh), DOMOWIK_OK, "close the hit-test plan");
  }

  /* ---- ownership abuse ---- */
  out.data = NULL;
  check_eq(plan_snapshot(h, &out), DOMOWIK_OK, "snapshot for the free probes");
  uint64_t issued = out.token;
  check_eq(buffer_free(issued), DOMOWIK_OK, "first free succeeds");
  check_eq(buffer_free(issued), DOMOWIK_UNKNOWN_BUFFER,
           "double free is REFUSED, not performed");

  /* Release takes a TOKEN, so there is no pointer for a caller to get wrong.
   * Any integer is safe to pass and is refused rather than acted on. */
  uint64_t forged_tokens[] = {0, 1, issued + 1, UINT64_MAX};
  for (size_t i = 0; i < sizeof forged_tokens / sizeof forged_tokens[0]; i++) {
    check_eq(buffer_free(forged_tokens[i]), DOMOWIK_UNKNOWN_BUFFER,
             "a forged token is refused");
  }

  /* Two EMPTY buffers share an address but must not share a token: a
   * pointer-keyed registry lost one of them. */
  domowik_buf e1 = {NULL, 0, 0}, e2 = {NULL, 0, 0};
  check_eq(last_error(h, &e1), DOMOWIK_OK, "empty error 1");
  check_eq(last_error(h, &e2), DOMOWIK_OK, "empty error 2");
  check(e1.len == 0 && e2.len == 0, "the fixture needs empty buffers");
  check(e1.token != e2.token, "empty buffers get distinct tokens");
  check_eq(buffer_free(e1.token), DOMOWIK_OK, "free the first empty buffer");
  check_eq(buffer_free(e2.token), DOMOWIK_OK, "free the second empty buffer");

  /* ---- forged and stale handles ---- */
  /* A SENTINEL, not zeroes. Initialising to zero and asserting `data == NULL`
   * afterwards is satisfied by an implementation that overwrote the whole
   * struct with zeroes — which would be a real bug that the check could not
   * see. Found by review. */
  domowik_buf sink;
  const uint8_t *sentinel_ptr = (const uint8_t *)(uintptr_t)0xA5A5A5A5u;
  uint64_t forged[] = {0, 999999, 0xdeadbeef, UINT64_MAX};
  for (size_t i = 0; i < sizeof forged / sizeof forged[0]; i++) {
    sink.data = (uint8_t *)(uintptr_t)0xA5A5A5A5u;
    sink.len = 0x5A5A5A5A;
    sink.token = 0x1234;
    check_eq(plan_snapshot(forged[i], &sink), DOMOWIK_UNKNOWN_HANDLE,
             "forged handle detected");
    check(sink.data == sentinel_ptr && sink.len == 0x5A5A5A5A &&
              sink.token == 0x1234,
          "a failed call leaves the out-parameter UNTOUCHED");
  }

  check_eq(plan_close(h), DOMOWIK_OK, "close");
  check_eq(plan_close(h), DOMOWIK_UNKNOWN_HANDLE, "double close is detected");
  check_eq(plan_snapshot(h, &sink), DOMOWIK_UNKNOWN_HANDLE,
           "a stale handle is detected");

  /* A new plan must not reuse the closed token. */
  uint64_t h2 = 0;
  check_eq(plan_create(&h2), DOMOWIK_OK, "second plan");
  check(h2 != h, "handles are never reissued");

  /* ---- malformed input, and the sticky error ---- */
  const char *junk = "{not json";
  check_eq(plan_execute(h2, (const uint8_t *)junk, strlen(junk), &sink),
           DOMOWIK_MALFORMED_REQUEST, "malformed request");
  out.data = NULL;
  check_eq(last_error(h2, &out), DOMOWIK_OK, "last_error");
  check(out.len > 0, "a reason was recorded");
  /* Snapshot it, then read again: retrieval must not clear or change it. */
  char first[512];
  size_t n = out.len < sizeof first - 1 ? out.len : sizeof first - 1;
  memcpy(first, out.data, n);
  first[n] = '\0';
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the error");
  out.data = NULL;
  check_eq(last_error(h2, &out), DOMOWIK_OK, "last_error again");
  check(out.len == n && memcmp(first, out.data, n) == 0,
        "the error is sticky and byte-identical on re-read");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the error again");

  /* ---- panic containment, across a FOREIGN frame ---- */
  check_eq(debug_panic(), DOMOWIK_PANIC, "panic is contained");
  /* And the library still works afterwards. A contained panic that leaves the
   * library unusable is a delayed crash, not containment. */
  uuid_like(uuid, sizeof uuid, 2);
  level_request(req, sizeof req, uuid);
  out.data = NULL;
  check_eq(plan_execute(h2, (const uint8_t *)req, strlen(req), &out),
           DOMOWIK_OK, "usable after a contained panic");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free after panic");

  /* ---- cancellation from another thread ---- */
  /* The Dart harness used to say this file covered the cross-thread case. It
   * did not: it ran `busy(h2, 1)` on the main thread and nothing else. Found
   * by review — evidence that claims coverage it does not have is worse than
   * a stated gap. */
  check_eq(busy(h2, 1), DOMOWIK_OK, "a short busy run completes");
  {
    struct cancel_args ca = {h2, -99};
    pthread_t t;
    if (pthread_create(&t, NULL, cancel_worker, &ca) != 0) {
      fprintf(stderr, "FAIL: pthread_create for cancel\n");
      return 1;
    }
    /* Let the long operation start, then cancel it from THIS thread. */
    struct timespec ts = {0, 20 * 1000 * 1000};
    nanosleep(&ts, NULL);
    check_eq(cancel(h2), DOMOWIK_OK, "cancel from another thread");
    pthread_join(t, NULL);
    check_eq(ca.result, DOMOWIK_CANCELED, "the long operation was cancelled");
  }

  /* ---- several OS threads, none created by Rust ---- */
  enum { THREADS = 8 };
  pthread_t threads[THREADS];
  struct worker_args args[THREADS];
  uint64_t shared = 0;
  check_eq(plan_create(&shared), DOMOWIK_OK, "shared plan");
  for (int i = 0; i < THREADS; i++) {
    args[i].handle = shared;
    args[i].seed = (unsigned)(100 + i);
    args[i].result = -99;
    if (pthread_create(&threads[i], NULL, worker, &args[i]) != 0) {
      fprintf(stderr, "FAIL: pthread_create\n");
      return 1;
    }
  }
  for (int i = 0; i < THREADS; i++) {
    pthread_join(threads[i], NULL);
    check_eq(args[i].result, DOMOWIK_OK, "threaded execute");
  }
  out.data = NULL;
  check_eq(plan_snapshot(shared, &out), DOMOWIK_OK, "snapshot after threads");
  /* Eight distinct levels, so nothing was lost to a race. */
  char *doc = as_cstring(&out);
  int levels = 0;
  for (const char *p = doc; (p = strstr(p, "\"name\":")); p += 7) {
    levels++;
  }
  free(doc);
  check(levels == THREADS, "every threaded command landed");
  check_eq(buffer_free(out.token), DOMOWIK_OK, "free the threaded snapshot");
  check_eq(plan_close(shared), DOMOWIK_OK, "close shared");
  check_eq(plan_close(h2), DOMOWIK_OK, "close h2");

  /* ---- nothing leaked ---- */
  check(outstanding_buffers() == 0, "no buffer was left outstanding");
  check(open_plans() == 0, "no plan was left open");

  printf("%d checks, %d failures\n", checks, failures);
  if (failures == 0) {
    printf("C HARNESS PASSED\n");
  }
  return failures == 0 ? 0 : 1;
}
