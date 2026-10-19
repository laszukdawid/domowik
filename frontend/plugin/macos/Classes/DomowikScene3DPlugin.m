// macOS presenter for the shared Rust 3D renderer (issue 0101).
//
// FRAME PATH. The Rust `wgpu` renderer draws straight into IOSurface-backed
// CVPixelBuffers, and Flutter samples the same IOSurface through
// `copyPixelBuffer`. No pixel crosses the CPU on a normal frame; the only
// readback is the explicit diagnostic one the probes request.
//
// BUFFER REUSE. Flutter's macOS embedder releases the CVPixelBuffer as soon
// as it has wrapped it in a Metal texture, then keeps painting that texture
// until the next `textureFrameAvailable`, and offers no consumer-completion
// signal (FlutterExternalTexture.mm, embedder_external_texture_metal.mm). The
// pool's own "released" therefore does not mean "Flutter is done". So every
// buffer we publish stays LEASED after a newer one replaces it, and returns to
// the pool only once Flutter has pulled two newer buffers (it no longer
// caches this one and has rastered past it) AND kLeaseSeconds have passed
// since the first of those pulls, timed per lease (the GPU work that sampled
// it has most likely finished). That is an inference, not a fence: the
// embedder exposes none, and the IOSurface retain count is held by Core
// Video's texture cache, so it is no signal either. A fresh surface per frame
// would avoid reuse entirely but costs about four times the acquisition time
// and a second of frames in memory (the cache ages entries out after ~1 s);
// performance decided for the lease (ED-0117). The residual risk is visual
// corruption of 3D frames, never memory safety: an IOSurface stays allocated
// while anything references it.
// Leases are capped across pool generations (kMaxLeases); when the cap or the
// pool threshold is reached the frame waits instead of allocating.
//
// THREADS. Method calls arrive on the main thread and never wait for the GPU:
// rendering, diagnostics and scene loads run on serial worker queues, and
// statistics are a snapshot Rust publishes after each frame. Teardown drains
// those queues on a background queue. `domowik_r3d_render` returns after the
// GPU finished; only then is the buffer published and Flutter told a frame is
// available. `copyPixelBuffer` (raster thread) only retains the latest
// published buffer and counts the pull.
//
// FAILURES. A failed frame or a permanent Core Video error becomes the
// session's failure and is pushed to Dart as a `health` call at once, so an
// idle view shows it too; the next successful frame pushes the recovery.
//
// SCENES. `loadScene` derives the scene from the plan session inside Rust;
// geometry never crosses Dart. Each load carries a generation and Rust
// refuses an older one that finishes late.

#import "DomowikScene3DPlugin.h"

#import <CoreVideo/CoreVideo.h>
#import <IOSurface/IOSurface.h>
#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>
#import <os/lock.h>

#include "domowik_bridge.h"

static NSString *const kChannel = @"invalid.domowik-spike/three-d";
static const double kLeaseSeconds = 0.1;
// Leases at 60 Hz for kLeaseSeconds plus two pulls, with headroom, across
// every pool generation; beyond this a frame waits for a lease to expire.
static const NSUInteger kMaxLeases = 10;
// Leases, the published and the in-flight buffer of the current pool.
static const int kPoolThreshold = 12;

static NSString *LastError(uint64_t handle) {
  domowik_buf buf = {0};
  if (domowik_r3d_last_error(handle, &buf) != DOMOWIK_OK) return @"unknown 3D renderer error";
  NSString *message = [[NSString alloc] initWithBytes:buf.data
                                               length:(NSUInteger)buf.len
                                             encoding:NSUTF8StringEncoding];
  domowik_buffer_free(buf.token);
  return message.length ? message : @"unknown 3D renderer error";
}

static id TakeJson(domowik_buf buf) {
  NSData *data = [NSData dataWithBytes:buf.data length:(NSUInteger)buf.len];
  domowik_buffer_free(buf.token);
  return [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
}

static FlutterError *Reject(NSString *message) {
  return [FlutterError errorWithCode:@"3d-renderer" message:message details:nil];
}

typedef struct {
  CVPixelBufferRef buffer;
  uint64_t pullsAtRelease;
  // When Flutter first pulled a newer buffer; 0 until then.
  CFTimeInterval firstNewerPullAt;
} Lease;

@interface DomowikSceneSession : NSObject <FlutterTexture>
@property(nonatomic, readonly) uint64_t renderer;
@property(nonatomic) int64_t textureId;
// Called on the main thread with a failure message, or nil on recovery.
@property(nonatomic, copy, nullable) void (^onHealth)(NSString *_Nullable failure);
@end

@implementation DomowikSceneSession {
  __weak NSObject<FlutterTextureRegistry> *_textures;
  id<MTLDevice> _device;
  dispatch_queue_t _renderQueue;
  dispatch_queue_t _sceneQueue;
  os_unfair_lock _lock;
  // Guarded by _lock.
  CVPixelBufferRef _latest;
  uint64_t _pulls;
  NSMutableData *_leases;  // Lease[]
  float _camera[16];
  int _width, _height;
  BOOL _dirty, _rendering, _closed, _identityCamera;
  uint64_t _cameraUpdates, _waits;
  int _waitStreak;
  NSString *_failure;
  // Render queue only.
  CVPixelBufferPoolRef _pool;
  int _poolWidth, _poolHeight;
  NSMutableDictionary<NSNumber *, NSNumber *> *_targets;
}

- (nullable instancetype)initWithTextures:(NSObject<FlutterTextureRegistry> *)textures
                                    error:(NSString *_Nullable *_Nonnull)error {
  if (!(self = [super init])) return nil;
  uint64_t handle = 0;
  if (domowik_r3d_create(&handle) != DOMOWIK_OK) {
    *error = LastError(0);
    return nil;
  }
  _renderer = handle;
  // Borrowed: the renderer owns the device and outlives every use here.
  _device = (__bridge id<MTLDevice>)domowik_r3d_metal_device(handle);
  if (!_device) {
    domowik_r3d_destroy(handle);
    *error = @"3D renderer has no Metal device";
    return nil;
  }
  _textures = textures;
  _renderQueue = dispatch_queue_create("domowik.3d.render", DISPATCH_QUEUE_SERIAL);
  _sceneQueue = dispatch_queue_create("domowik.3d.scene", DISPATCH_QUEUE_SERIAL);
  _lock = OS_UNFAIR_LOCK_INIT;
  _leases = [NSMutableData data];
  _width = 800;
  _height = 600;
  _targets = [NSMutableDictionary dictionary];
  for (int i = 0; i < 16; i++) _camera[i] = (i % 5 == 0) ? 1.f : 0.f;
  return self;
}

- (CVPixelBufferRef)copyPixelBuffer {
  os_unfair_lock_lock(&_lock);
  CVPixelBufferRef buffer = _latest ? CVPixelBufferRetain(_latest) : NULL;
  if (buffer) {
    _pulls++;
    // Date each lease's first newer pull. At most kMaxLeases entries.
    CFTimeInterval now = CACurrentMediaTime();
    Lease *leases = _leases.mutableBytes;
    for (NSUInteger i = 0, n = _leases.length / sizeof(Lease); i < n; i++) {
      if (leases[i].firstNewerPullAt == 0) leases[i].firstNewerPullAt = now;
    }
  }
  os_unfair_lock_unlock(&_lock);
  return buffer;
}

- (void)setCamera:(const double *)matrix width:(int)width height:(int)height {
  os_unfair_lock_lock(&_lock);
  for (int i = 0; i < 16; i++) _camera[i] = (float)matrix[i];
  _width = MAX(32, MIN(4096, width));
  _height = MAX(32, MIN(4096, height));
  _cameraUpdates++;
  os_unfair_lock_unlock(&_lock);
  [self requestFrame];
}

- (void)setIdentityCamera:(BOOL)identity {
  os_unfair_lock_lock(&_lock);
  _identityCamera = identity;
  os_unfair_lock_unlock(&_lock);
}

// Records a frame failure and tells Dart, which shows it until a later frame
// succeeds and reports recovery.
- (void)fail:(NSString *)message {
  os_unfair_lock_lock(&_lock);
  _failure = message;
  os_unfair_lock_unlock(&_lock);
  [self reportHealth:message];
}

- (void)reportHealth:(nullable NSString *)failure {
  void (^health)(NSString *) = self.onHealth;
  if (!health) return;
  dispatch_async(dispatch_get_main_queue(), ^{
    health(failure);
  });
}

// Coalesces: any number of requests while a frame renders become one more
// frame with the latest camera.
- (void)requestFrame {
  os_unfair_lock_lock(&_lock);
  _dirty = YES;
  BOOL start = !_rendering && !_closed;
  if (start) _rendering = YES;
  os_unfair_lock_unlock(&_lock);
  if (start) dispatch_async(_renderQueue, ^{ [self renderPending]; });
}

- (void)notifyFlutter {
  int64_t textureId = self.textureId;
  __weak NSObject<FlutterTextureRegistry> *textures = _textures;
  dispatch_async(dispatch_get_main_queue(), ^{
    [textures textureFrameAvailable:textureId];
  });
}

// Returns expired leases to the pool and reports how many remain. Render
// queue.
- (NSUInteger)expireLeases {
  CFTimeInterval now = CACurrentMediaTime();
  NSMutableArray *expired = [NSMutableArray array];
  os_unfair_lock_lock(&_lock);
  Lease *leases = _leases.mutableBytes;
  NSUInteger count = _leases.length / sizeof(Lease), kept = 0;
  for (NSUInteger i = 0; i < count; i++) {
    BOOL pulledPast = _pulls >= leases[i].pullsAtRelease + 2;
    BOOL aged = leases[i].firstNewerPullAt > 0 &&
                now - leases[i].firstNewerPullAt >= kLeaseSeconds;
    if (pulledPast && aged) {
      [expired addObject:(__bridge_transfer id)leases[i].buffer];
    } else {
      leases[kept++] = leases[i];
    }
  }
  _leases.length = kept * sizeof(Lease);
  os_unfair_lock_unlock(&_lock);
  // `expired` drops the pool references when it goes out of scope.
  return kept;
}

- (void)renderPending {
  for (;;) {
    NSUInteger leased = [self expireLeases];
    os_unfair_lock_lock(&_lock);
    if (!_dirty || _closed) {
      _rendering = NO;
      os_unfair_lock_unlock(&_lock);
      return;
    }
    _dirty = NO;
    float camera[16];
    memcpy(camera, _camera, sizeof camera);
    BOOL identity = _identityCamera;
    int width = _width, height = _height;
    os_unfair_lock_unlock(&_lock);
    if (identity) {
      for (int i = 0; i < 16; i++) camera[i] = (i % 5 == 0) ? 1.f : 0.f;
    }
    CVPixelBufferRef buffer = NULL;
    CVReturn result = leased >= kMaxLeases
                          ? kCVReturnWouldExceedAllocationThreshold
                          : [self nextBuffer:&buffer width:width height:height];
    if (result == kCVReturnWouldExceedAllocationThreshold) {
      // Every buffer is leased. Ask Flutter to pull again, which lets leases
      // age out, and retry with backoff: 4 ms doubling to 256 ms, so a hidden
      // view (which Flutter does not pull) costs almost nothing.
      os_unfair_lock_lock(&_lock);
      _waits++;
      int64_t delay = (int64_t)(4 * NSEC_PER_MSEC) << MIN(_waitStreak, 6);
      _waitStreak++;
      _dirty = YES;
      _rendering = NO;
      os_unfair_lock_unlock(&_lock);
      [self notifyFlutter];
      dispatch_after(dispatch_time(DISPATCH_TIME_NOW, delay), _renderQueue, ^{
        [self requestFrame];
      });
      return;
    }
    if (result != kCVReturnSuccess) {
      // Permanent: not retried until the next request.
      [self fail:[NSString stringWithFormat:@"3D buffer allocation failed (CVReturn %d)", result]];
      continue;
    }
    uint64_t target = [self targetFor:buffer width:width height:height];
    if (!target || domowik_r3d_render(_renderer, target, camera, identity ? 0 : 1) != DOMOWIK_OK) {
      [self fail:target ? LastError(_renderer) : @"3D target import failed"];
      CVPixelBufferRelease(buffer);
      continue;
    }
    os_unfair_lock_lock(&_lock);
    CVPixelBufferRef old = _latest;
    _latest = buffer;  // Our pool reference moves into _latest.
    if (old) {
      Lease lease = {old, _pulls, 0};
      [_leases appendBytes:&lease length:sizeof lease];
    }
    BOOL recovered = _failure != nil;
    _failure = nil;
    _waitStreak = 0;
    os_unfair_lock_unlock(&_lock);
    if (recovered) [self reportHealth:nil];
    [self notifyFlutter];
  }
}

- (CVReturn)nextBuffer:(CVPixelBufferRef *)buffer width:(int)width height:(int)height {
  if (!_pool || _poolWidth != width || _poolHeight != height) {
    [self releasePool];
    NSDictionary *pixel = @{
      (id)kCVPixelBufferPixelFormatTypeKey : @(kCVPixelFormatType_32BGRA),
      (id)kCVPixelBufferWidthKey : @(width),
      (id)kCVPixelBufferHeightKey : @(height),
      (id)kCVPixelBufferIOSurfacePropertiesKey : @{},
      (id)kCVPixelBufferMetalCompatibilityKey : @YES,
    };
    NSDictionary *pool = @{(id)kCVPixelBufferPoolMinimumBufferCountKey : @3};
    CVReturn created = CVPixelBufferPoolCreate(kCFAllocatorDefault, (__bridge CFDictionaryRef)pool,
                                               (__bridge CFDictionaryRef)pixel, &_pool);
    if (created != kCVReturnSuccess) {
      _pool = NULL;
      return created;
    }
    _poolWidth = width;
    _poolHeight = height;
  }
  NSDictionary *aux = @{(id)kCVPixelBufferPoolAllocationThresholdKey : @(kPoolThreshold)};
  return CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(
      kCFAllocatorDefault, _pool, (__bridge CFDictionaryRef)aux, buffer);
}

// One renderer target per pooled IOSurface, created on first use. The pool
// reuses the same surfaces, so steady-state frames import nothing.
- (uint64_t)targetFor:(CVPixelBufferRef)buffer width:(int)width height:(int)height {
  IOSurfaceRef surface = CVPixelBufferGetIOSurface(buffer);
  if (!surface) return 0;
  NSNumber *key = @(IOSurfaceGetID(surface));
  NSNumber *known = _targets[key];
  if (known) return known.unsignedLongLongValue;
  MTLTextureDescriptor *desc =
      [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:MTLPixelFormatBGRA8Unorm
                                                         width:(NSUInteger)width
                                                        height:(NSUInteger)height
                                                     mipmapped:NO];
  desc.usage = MTLTextureUsageRenderTarget | MTLTextureUsageShaderRead;
  desc.storageMode = MTLStorageModeShared;
  id<MTLTexture> texture = [_device newTextureWithDescriptor:desc iosurface:surface plane:0];
  if (!texture) return 0;
  uint64_t target = 0;
  // The +1 retain moves into Rust, which releases it on every path.
  if (domowik_r3d_attach_target(_renderer, (__bridge_retained void *)texture, (uint32_t)width,
                                (uint32_t)height, &target) != DOMOWIK_OK) {
    return 0;
  }
  _targets[key] = @(target);
  return target;
}

// Render queue. Leased and published buffers from the old pool stay valid:
// they hold their own references and are released through the lease path.
- (void)releasePool {
  for (NSNumber *target in _targets.allValues) {
    domowik_r3d_detach_target(_renderer, target.unsignedLongLongValue);
  }
  [_targets removeAllObjects];
  if (_pool) {
    CVPixelBufferPoolFlush(_pool, kCVPixelBufferPoolFlushExcessBuffers);
    CVPixelBufferPoolRelease(_pool);
    _pool = NULL;
  }
}

- (void)loadScene:(NSDictionary *)args reply:(FlutterResult)reply {
  NSNumber *plan = args[@"plan"], *generation = args[@"generation"];
  NSDictionary *request = @{
    @"levelIds" : args[@"levelIds"] ?: @[],
    @"ghostedIds" : args[@"ghostedIds"] ?: @[],
    @"poseOnly" : args[@"poseOnly"] ?: @NO,
    @"sceneToken" : args[@"sceneToken"] ?: [NSNull null],
  };
  NSData *body = [NSJSONSerialization dataWithJSONObject:request options:0 error:nil];
  if (![plan isKindOfClass:NSNumber.class] || ![generation isKindOfClass:NSNumber.class] || !body) {
    reply(Reject(@"Invalid scene request"));
    return;
  }
  uint64_t renderer = _renderer;
  dispatch_async(_sceneQueue, ^{
    domowik_buf out = {0};
    int32_t code = domowik_r3d_load_scene(renderer, plan.unsignedLongLongValue, body.bytes,
                                          body.length, generation.unsignedLongLongValue, &out);
    id result = code == DOMOWIK_OK ? TakeJson(out) : nil;
    NSString *error = code == DOMOWIK_OK ? nil : LastError(renderer);
    dispatch_async(dispatch_get_main_queue(), ^{
      if (result) {
        [self setIdentityCamera:NO];
        [self requestFrame];
        reply(result);
      } else {
        reply(Reject(error));
      }
    });
  });
}

// Diagnostic: the orientation fixture replaces the scene on the render queue,
// so the main thread never waits for the renderer.
- (void)probeOrientation:(FlutterResult)reply {
  uint64_t renderer = _renderer;
  dispatch_async(_renderQueue, ^{
    BOOL ok = domowik_r3d_orientation_probe(renderer) == DOMOWIK_OK;
    NSString *error = ok ? nil : LastError(renderer);
    dispatch_async(dispatch_get_main_queue(), ^{
      if (!ok) {
        reply(Reject(error));
        return;
      }
      [self setIdentityCamera:YES];
      [self requestFrame];
      reply(nil);
    });
  });
}

// Never waits for the GPU: Rust's statistics are a published snapshot.
- (NSDictionary *)stats {
  domowik_buf out = {0};
  if (domowik_r3d_stats(_renderer, &out) != DOMOWIK_OK) return @{};
  NSMutableDictionary *stats = [TakeJson(out) mutableCopy] ?: [NSMutableDictionary dictionary];
  os_unfair_lock_lock(&_lock);
  stats[@"cameraUpdates"] = @(_cameraUpdates);
  stats[@"consumerPulls"] = @(_pulls);
  stats[@"leases"] = @(_leases.length / sizeof(Lease));
  stats[@"poolWaits"] = @(_waits);
  // `error` is CURRENT health; Rust's `lastRefusal` keeps history.
  stats[@"error"] = _failure ?: @"";
  os_unfair_lock_unlock(&_lock);
  return stats;
}

// Blocking teardown; call it off the main thread. Stops new frames, drains
// the scene and render queues, then releases every GPU and Core Video
// resource before the renderer.
- (void)close {
  os_unfair_lock_lock(&_lock);
  _closed = YES;
  os_unfair_lock_unlock(&_lock);
  dispatch_sync(_sceneQueue, ^{});
  dispatch_sync(_renderQueue, ^{
    [self releasePool];
  });
  os_unfair_lock_lock(&_lock);
  CVPixelBufferRef latest = _latest;
  _latest = NULL;
  Lease *leases = _leases.mutableBytes;
  NSUInteger count = _leases.length / sizeof(Lease);
  NSMutableArray *held = [NSMutableArray arrayWithCapacity:count];
  for (NSUInteger i = 0; i < count; i++) [held addObject:(__bridge_transfer id)leases[i].buffer];
  _leases.length = 0;
  os_unfair_lock_unlock(&_lock);
  if (latest) CVPixelBufferRelease(latest);
  [held removeAllObjects];
  domowik_r3d_destroy(_renderer);
  _renderer = 0;
}
@end

@implementation DomowikScene3DPlugin {
  NSObject<FlutterTextureRegistry> *_textures;
  FlutterMethodChannel *_channel;
  NSMutableDictionary<NSNumber *, DomowikSceneSession *> *_sessions;
  uint64_t _nextSession;
}

+ (void)registerWithRegistrar:(NSObject<FlutterPluginRegistrar> *)registrar {
  DomowikScene3DPlugin *plugin = [[DomowikScene3DPlugin alloc] init];
  plugin->_textures = registrar.textures;
  plugin->_sessions = [NSMutableDictionary dictionary];
  plugin->_nextSession = 1;
  FlutterMethodChannel *channel = [FlutterMethodChannel methodChannelWithName:kChannel
                                                              binaryMessenger:registrar.messenger];
  plugin->_channel = channel;
  [registrar addMethodCallDelegate:plugin channel:channel];
}

// Unregisters at once, so Flutter stops pulling, and drains off the main
// thread. The block keeps the session alive until its teardown finishes.
static void CloseInBackground(NSObject<FlutterTextureRegistry> *textures,
                              DomowikSceneSession *session) {
  [textures unregisterTexture:session.textureId];
  dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
    [session close];
  });
}

- (void)handleMethodCall:(FlutterMethodCall *)call result:(FlutterResult)result {
  NSDictionary *args = [call.arguments isKindOfClass:NSDictionary.class] ? call.arguments : @{};
  if ([call.method isEqualToString:@"create"]) {
    NSString *error = nil;
    DomowikSceneSession *session = [[DomowikSceneSession alloc] initWithTextures:_textures
                                                                           error:&error];
    if (!session) {
      result(Reject(error));
      return;
    }
    session.textureId = [_textures registerTexture:session];
    NSNumber *key = @(_nextSession++);
    _sessions[key] = session;
    // Frame failures and recoveries reach Dart without waiting for input.
    __weak FlutterMethodChannel *channel = _channel;
    session.onHealth = ^(NSString *failure) {
      [channel invokeMethod:@"health"
                  arguments:@{@"session" : key, @"error" : failure ?: [NSNull null]}];
    };
    // `loadScene` tells the Dart client this presenter pulls scenes from Rust.
    result(@{@"textureId" : @(session.textureId), @"session" : key, @"loadScene" : @YES});
    return;
  }
  DomowikSceneSession *session = _sessions[args[@"session"] ?: @0];
  if (!session) {
    result(Reject(@"Unknown 3D session"));
    return;
  }
  if ([call.method isEqualToString:@"destroy"]) {
    [_sessions removeObjectForKey:args[@"session"]];
    CloseInBackground(_textures, session);
    result(nil);
  } else if ([call.method isEqualToString:@"camera"]) {
    FlutterStandardTypedData *matrix = args[@"matrix"];
    NSNumber *width = args[@"width"], *height = args[@"height"];
    if (![matrix isKindOfClass:FlutterStandardTypedData.class] ||
        matrix.type != FlutterStandardDataTypeFloat64 || matrix.elementCount != 16 ||
        ![width isKindOfClass:NSNumber.class] || ![height isKindOfClass:NSNumber.class]) {
      result(Reject(@"Invalid camera matrix"));
      return;
    }
    const double *m = matrix.data.bytes;
    for (int i = 0; i < 16; i++) {
      if (!isfinite(m[i])) {
        result(Reject(@"Invalid camera matrix"));
        return;
      }
    }
    [session setCamera:m width:width.intValue height:height.intValue];
    result(nil);
  } else if ([call.method isEqualToString:@"loadScene"]) {
    [session loadScene:args reply:result];
  } else if ([call.method isEqualToString:@"reset"]) {
    // Scene replacement is atomic in loadScene; nothing to reset.
    result(nil);
  } else if ([call.method isEqualToString:@"stats"]) {
    result([session stats]);
  } else if ([call.method isEqualToString:@"diagnosticCapture"]) {
    domowik_r3d_request_diagnostic(session.renderer);
    [session requestFrame];
    result(nil);
  } else if ([call.method isEqualToString:@"orientationProbe"]) {
    [session probeOrientation:result];
  } else if ([call.method isEqualToString:@"orientationStats"]) {
    NSDictionary *stats = [session stats];
    result(@{
      @"topRedPixels" : stats[@"topRedPixels"] ?: @0,
      @"bottomBluePixels" : stats[@"bottomBluePixels"] ?: @0,
    });
  } else {
    // `mesh` and `scene` belong to the legacy push path (Linux); this
    // presenter loads scenes from Rust instead.
    result(FlutterMethodNotImplemented);
  }
}

- (void)detachFromEngineForRegistrar:(NSObject<FlutterPluginRegistrar> *)registrar {
  for (DomowikSceneSession *session in _sessions.allValues) {
    CloseInBackground(_textures, session);
  }
  [_sessions removeAllObjects];
}
@end
