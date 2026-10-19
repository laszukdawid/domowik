// Original native GPU texture experiment, ADR 0010 / issue 0078.
#include "scene_texture.h"
#include <epoxy/gl.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <map>
#include <mutex>
#include <string>
#include <vector>

namespace {
using Clock = std::chrono::steady_clock;
struct Mesh {
  std::vector<float> vertices; GLuint buffer=0;
  std::vector<float> coordinates; std::vector<uint8_t> rgba;
  int width=1,height=1,layers=1; GLuint uv_buffer=0,image=0;
  Mesh(std::vector<float> v={},GLuint b=0):vertices(std::move(v)),buffer(b){}
};
struct Group { std::string mesh; bool transparent=false; std::vector<float> instances; GLuint vao=0,buffer=0; };
struct Renderer {
  std::mutex mutex;
  std::mutex camera_mutex;
  std::map<std::string,Mesh> meshes;
  std::vector<Group> groups;
  std::array<float,16> camera{};
  GLuint program=0,texture=0,framebuffer=0,depth=0;
  int width=800,height=600,allocated_width=0,allocated_height=0;
  int requested_width=800,requested_height=600;
  bool scene_dirty=true;
  uint64_t frames=0,mesh_uploads=0,instance_uploads=0;
  std::atomic<uint64_t> camera_updates{0};
  uint64_t scene_updates=0,triangles=0,draw_calls=0;
  double render_us=0;
  std::string driver,error;
  GdkGLContext* context=nullptr; // identity only, not an owning context reference
  bool diagnostic_readback=false;
  uint64_t diagnostic_pixels=0;
  uint64_t orientation_top_red=0,orientation_bottom_blue=0;
};
std::mutex retired_mutex;
std::vector<Renderer*> retired;

void clear_gpu(Renderer* r) {
  for(auto& [_,m]:r->meshes) {if(m.buffer)glDeleteBuffers(1,&m.buffer);if(m.uv_buffer)glDeleteBuffers(1,&m.uv_buffer);if(m.image)glDeleteTextures(1,&m.image);m.buffer=m.uv_buffer=m.image=0;}
  for(auto& g:r->groups) {if(g.buffer)glDeleteBuffers(1,&g.buffer);if(g.vao)glDeleteVertexArrays(1,&g.vao);g.buffer=g.vao=0;}
  if(r->program)glDeleteProgram(r->program);
  if(r->texture)glDeleteTextures(1,&r->texture);
  if(r->framebuffer)glDeleteFramebuffers(1,&r->framebuffer);
  if(r->depth)glDeleteRenderbuffers(1,&r->depth);
  r->program=r->texture=r->framebuffer=r->depth=0;r->allocated_width=0;
}

GLuint shader(GLenum kind,const std::string& text,std::string& error) {
  GLuint shader=glCreateShader(kind);const char* source=text.c_str();glShaderSource(shader,1,&source,nullptr);glCompileShader(shader);
  GLint good=0;glGetShaderiv(shader,GL_COMPILE_STATUS,&good);
  if(!good){char log[4096];GLsizei length=0;glGetShaderInfoLog(shader,sizeof(log),&length,log);error.assign(log,length);glDeleteShader(shader);return 0;}
  return shader;
}

bool initialize(Renderer* r) {
  const std::string prelude=epoxy_is_desktop_gl()?"#version 150\n":"#version 300 es\nprecision highp float;\n";
  GLuint vertex=shader(GL_VERTEX_SHADER,prelude+R"(
in vec3 aPosition; in vec3 aNormal; in vec4 aColor;
in vec3 aUvLayer; in vec2 aWrap; out vec2 vUv; flat out int vLayer; flat out vec2 vWrap;
in vec4 iM0; in vec4 iM1; in vec4 iM2; in vec4 iM3;
in vec3 iN0; in vec3 iN1; in vec3 iN2; in vec4 iColor;
uniform mat4 uCamera; out vec3 vNormal; out vec4 vColor;
// OpenGL framebuffer rows are bottom-origin; Flutter samples this external
// texture with top-origin presentation. Reflect clip Y, not world/camera data.
void main(){mat4 m=mat4(iM0,iM1,iM2,iM3);gl_Position=uCamera*m*vec4(aPosition,1.0);gl_Position.y=-gl_Position.y;
vNormal=mat3(iN0,iN1,iN2)*aNormal;vColor=aColor*iColor;vUv=aUvLayer.xy;vLayer=int(aUvLayer.z);vWrap=aWrap;}
)",r->error);
  GLuint fragment=shader(GL_FRAGMENT_SHADER,prelude+R"(
in vec3 vNormal; in vec4 vColor; out vec4 color;
in vec2 vUv; flat in int vLayer; flat in vec2 vWrap;
uniform highp sampler2DArray uBaseColor;
void main(){vec2 uv=vUv;vec2 halfTexel=0.5/vec2(textureSize(uBaseColor,0).xy);
for(int i=0;i<2;i++){if(vWrap[i]==1.0)uv[i]=clamp(uv[i],halfTexel[i],1.0-halfTexel[i]);if(vWrap[i]==2.0)uv[i]=clamp(1.0-abs(mod(uv[i],2.0)-1.0),halfTexel[i],1.0-halfTexel[i]);}
vec4 texel=texture(uBaseColor,vec3(uv,float(max(vLayer,0))));vec4 base=vColor;
if(vLayer>=0){vec3 linear=mix(pow((base.rgb+0.055)/1.055,vec3(2.4)),base.rgb/12.92,lessThanEqual(base.rgb,vec3(0.04045)))*texel.rgb;
base=vec4(mix(1.055*pow(linear,vec3(1.0/2.4))-0.055,linear*12.92,lessThanEqual(linear,vec3(0.0031308))),base.a*texel.a);}
if(base.a<0.01)discard;vec3 n=normalize(vNormal);if(!gl_FrontFacing)n=-n;
float light=0.38+0.62*max(dot(n,normalize(vec3(0.35,0.85,0.4))),0.0);
color=vec4(base.rgb*light,base.a);}
)",r->error);
  if(!vertex||!fragment){if(vertex)glDeleteShader(vertex);if(fragment)glDeleteShader(fragment);return false;}
  r->program=glCreateProgram();glAttachShader(r->program,vertex);glAttachShader(r->program,fragment);
  const char* attrs[]={"aPosition","aNormal","aColor","iM0","iM1","iM2","iM3","iN0","iN1","iN2","iColor","aUvLayer","aWrap"};
  for(GLuint i=0;i<13;i++)glBindAttribLocation(r->program,i,attrs[i]);
  glLinkProgram(r->program);glDeleteShader(vertex);glDeleteShader(fragment);
  GLint good=0;glGetProgramiv(r->program,GL_LINK_STATUS,&good);
  if(!good){char log[4096];GLsizei length=0;glGetProgramInfoLog(r->program,sizeof(log),&length,log);r->error.assign(log,length);glDeleteProgram(r->program);r->program=0;return false;}
  const auto* driver=glGetString(GL_RENDERER);r->driver=driver?reinterpret_cast<const char*>(driver):"unknown";
  return true;
}

void upload(Renderer* r) {
  glActiveTexture(GL_TEXTURE0);
  glBindBuffer(GL_PIXEL_UNPACK_BUFFER,0);glPixelStorei(GL_UNPACK_ALIGNMENT,1);glPixelStorei(GL_UNPACK_ROW_LENGTH,0);glPixelStorei(GL_UNPACK_IMAGE_HEIGHT,0);glPixelStorei(GL_UNPACK_SKIP_PIXELS,0);glPixelStorei(GL_UNPACK_SKIP_ROWS,0);glPixelStorei(GL_UNPACK_SKIP_IMAGES,0);
  for(auto& [_,m]:r->meshes) {if(m.buffer==0){glGenBuffers(1,&m.buffer);glBindBuffer(GL_ARRAY_BUFFER,m.buffer);glBufferData(GL_ARRAY_BUFFER,m.vertices.size()*sizeof(float),m.vertices.data(),GL_STATIC_DRAW);r->mesh_uploads++;
    std::vector<float> fallback;if(m.coordinates.empty()){for(size_t i=0;i<m.vertices.size()/10;i++)fallback.insert(fallback.end(),{0.f,0.f,-1.f,0.f,0.f});}
    const auto& coords=m.coordinates.empty()?fallback:m.coordinates;
    glGenBuffers(1,&m.uv_buffer);glBindBuffer(GL_ARRAY_BUFFER,m.uv_buffer);glBufferData(GL_ARRAY_BUFFER,coords.size()*sizeof(float),coords.data(),GL_STATIC_DRAW);
    const uint8_t white[]={255,255,255,255};glGenTextures(1,&m.image);glBindTexture(GL_TEXTURE_2D_ARRAY,m.image);glTexParameteri(GL_TEXTURE_2D_ARRAY,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D_ARRAY,GL_TEXTURE_MAG_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D_ARRAY,GL_TEXTURE_WRAP_S,GL_REPEAT);glTexParameteri(GL_TEXTURE_2D_ARRAY,GL_TEXTURE_WRAP_T,GL_REPEAT);glTexImage3D(GL_TEXTURE_2D_ARRAY,0,GL_SRGB8_ALPHA8,m.width,m.height,m.layers,0,GL_RGBA,GL_UNSIGNED_BYTE,m.rgba.empty()?white:m.rgba.data());
  }}
  if(!r->scene_dirty)return;
  for(auto& g:r->groups) {
    if(g.vao)glDeleteVertexArrays(1,&g.vao);if(g.buffer)glDeleteBuffers(1,&g.buffer);g.vao=g.buffer=0;
    glGenVertexArrays(1,&g.vao);glBindVertexArray(g.vao);
    auto mesh=r->meshes.find(g.mesh);if(mesh==r->meshes.end())continue;
    glBindBuffer(GL_ARRAY_BUFFER,mesh->second.buffer);
    for(GLuint i=0;i<3;i++){glEnableVertexAttribArray(i);glVertexAttribPointer(i,i==2?4:3,GL_FLOAT,GL_FALSE,10*sizeof(float),reinterpret_cast<void*>((i==0?0:i==1?3:6)*sizeof(float)));}
    glBindBuffer(GL_ARRAY_BUFFER,mesh->second.uv_buffer);glEnableVertexAttribArray(11);glVertexAttribPointer(11,3,GL_FLOAT,GL_FALSE,5*sizeof(float),nullptr);glEnableVertexAttribArray(12);glVertexAttribPointer(12,2,GL_FLOAT,GL_FALSE,5*sizeof(float),reinterpret_cast<void*>(3*sizeof(float)));
    glGenBuffers(1,&g.buffer);glBindBuffer(GL_ARRAY_BUFFER,g.buffer);glBufferData(GL_ARRAY_BUFFER,g.instances.size()*sizeof(float),g.instances.data(),GL_STATIC_DRAW);
    for(GLuint i=3;i<11;i++){const int offset=i<7?(i-3)*4:i<10?16+(i-7)*3:25;glEnableVertexAttribArray(i);glVertexAttribPointer(i,i<7||i==10?4:3,GL_FLOAT,GL_FALSE,29*sizeof(float),reinterpret_cast<void*>(offset*sizeof(float)));glVertexAttribDivisor(i,1);}
    r->instance_uploads++;
  }
  r->scene_dirty=false;
}

struct GlState {
  GLint fbo=0,read_fbo=0,program=0,vao=0,buffer=0,renderbuffer=0,viewport[4]{},active=0,texture=0,depth_func=0,blend_src=0,blend_dst=0,blend_src_alpha=0,blend_dst_alpha=0;
  GLboolean depth_test,blend,scissor,cull,stencil,depth_mask,color_mask[4]{};
  GLfloat clear_color[4]{},clear_depth=1.f,depth_range[2]{0.f,1.f};
  GlState(){glGetIntegerv(GL_FRAMEBUFFER_BINDING,&fbo);glGetIntegerv(GL_READ_FRAMEBUFFER_BINDING,&read_fbo);glGetIntegerv(GL_CURRENT_PROGRAM,&program);glGetIntegerv(GL_VERTEX_ARRAY_BINDING,&vao);glGetIntegerv(GL_ARRAY_BUFFER_BINDING,&buffer);glGetIntegerv(GL_RENDERBUFFER_BINDING,&renderbuffer);glGetIntegerv(GL_VIEWPORT,viewport);glGetIntegerv(GL_ACTIVE_TEXTURE,&active);glGetIntegerv(GL_TEXTURE_BINDING_2D,&texture);glGetIntegerv(GL_DEPTH_FUNC,&depth_func);glGetIntegerv(GL_BLEND_SRC_RGB,&blend_src);glGetIntegerv(GL_BLEND_DST_RGB,&blend_dst);glGetIntegerv(GL_BLEND_SRC_ALPHA,&blend_src_alpha);glGetIntegerv(GL_BLEND_DST_ALPHA,&blend_dst_alpha);glGetBooleanv(GL_DEPTH_WRITEMASK,&depth_mask);glGetBooleanv(GL_COLOR_WRITEMASK,color_mask);glGetFloatv(GL_COLOR_CLEAR_VALUE,clear_color);glGetFloatv(GL_DEPTH_CLEAR_VALUE,&clear_depth);glGetFloatv(GL_DEPTH_RANGE,depth_range);depth_test=glIsEnabled(GL_DEPTH_TEST);blend=glIsEnabled(GL_BLEND);scissor=glIsEnabled(GL_SCISSOR_TEST);cull=glIsEnabled(GL_CULL_FACE);stencil=glIsEnabled(GL_STENCIL_TEST);}
  ~GlState(){glBindFramebuffer(GL_DRAW_FRAMEBUFFER,fbo);glBindFramebuffer(GL_READ_FRAMEBUFFER,read_fbo);glUseProgram(program);glBindVertexArray(vao);glBindBuffer(GL_ARRAY_BUFFER,buffer);glBindRenderbuffer(GL_RENDERBUFFER,renderbuffer);glViewport(viewport[0],viewport[1],viewport[2],viewport[3]);glActiveTexture(active);glBindTexture(GL_TEXTURE_2D,texture);glDepthFunc(depth_func);glDepthMask(depth_mask);glClearDepthf(clear_depth);glDepthRangef(depth_range[0],depth_range[1]);glColorMask(color_mask[0],color_mask[1],color_mask[2],color_mask[3]);glBlendFuncSeparate(blend_src,blend_dst,blend_src_alpha,blend_dst_alpha);glClearColor(clear_color[0],clear_color[1],clear_color[2],clear_color[3]);for(auto p:{std::pair<GLenum,GLboolean>{GL_DEPTH_TEST,depth_test},{GL_BLEND,blend},{GL_SCISSOR_TEST,scissor},{GL_CULL_FACE,cull},{GL_STENCIL_TEST,stencil}}){if(p.second)glEnable(p.first);else glDisable(p.first);}}
};
struct FrontFaceState {
  GLint previous=GL_CCW;
  // Clip-Y reflection reverses projected winding. Keep gl_FrontFacing aligned
  // with the unchanged world-space normals used for two-sided lighting.
  FrontFaceState(){glGetIntegerv(GL_FRONT_FACE,&previous);glFrontFace(GL_CW);}
  ~FrontFaceState(){glFrontFace(previous);}
};
// Flutter shares this GL context. Preserve the unit-0 array binding and every
// pixel-unpack setting touched by model uploads, including any host PBO.
struct TextureState {
  GLint active=0,array=0,buffer=0,sampler=0,values[6]{};
  const bool sampler_objects=!epoxy_is_desktop_gl()||epoxy_gl_version()>=33||epoxy_has_gl_extension("GL_ARB_sampler_objects");
  const GLenum names[6]={GL_UNPACK_ALIGNMENT,GL_UNPACK_ROW_LENGTH,GL_UNPACK_IMAGE_HEIGHT,GL_UNPACK_SKIP_PIXELS,GL_UNPACK_SKIP_ROWS,GL_UNPACK_SKIP_IMAGES};
  TextureState(){glGetIntegerv(GL_ACTIVE_TEXTURE,&active);glActiveTexture(GL_TEXTURE0);glGetIntegerv(GL_TEXTURE_BINDING_2D_ARRAY,&array);if(sampler_objects){glGetIntegerv(GL_SAMPLER_BINDING,&sampler);glBindSampler(0,0);}glGetIntegerv(GL_PIXEL_UNPACK_BUFFER_BINDING,&buffer);for(int i=0;i<6;i++)glGetIntegerv(names[i],&values[i]);glActiveTexture(active);}
  ~TextureState(){glActiveTexture(GL_TEXTURE0);glBindTexture(GL_TEXTURE_2D_ARRAY,array);if(sampler_objects)glBindSampler(0,sampler);glBindBuffer(GL_PIXEL_UNPACK_BUFFER,buffer);for(int i=0;i<6;i++)glPixelStorei(names[i],values[i]);glActiveTexture(active);}
};
}

typedef struct _DomowikSceneTexture {FlTextureGL parent;Renderer* renderer;} DomowikSceneTexture;
typedef struct _DomowikSceneTextureClass {FlTextureGLClass parent;} DomowikSceneTextureClass;
G_DEFINE_TYPE(DomowikSceneTexture,domowik_scene_texture,fl_texture_gl_get_type())

static gboolean populate(FlTextureGL* base,uint32_t* target,uint32_t* name,uint32_t* width,uint32_t* height,GError** error) {
  auto* r=reinterpret_cast<DomowikSceneTexture*>(base)->renderer;
  std::lock_guard<std::mutex> lock(r->mutex);GlState restore;FrontFaceState front_face;TextureState texture_state;
  std::array<float,16> camera;
  {std::lock_guard<std::mutex> camera_lock(r->camera_mutex);camera=r->camera;r->width=r->requested_width;r->height=r->requested_height;}
  GdkGLContext* current=gdk_gl_context_get_current();
  {std::lock_guard<std::mutex> retired_lock(retired_mutex);for(auto* old:retired){if(old->context==current)clear_gpu(old);delete old;}retired.clear();}
  if(r->context!=current){r->context=current;r->program=r->texture=r->framebuffer=r->depth=0;r->allocated_width=0;for(auto& [_,m]:r->meshes)m.buffer=m.uv_buffer=m.image=0;for(auto& g:r->groups)g.vao=g.buffer=0;r->scene_dirty=true;}
  const auto start=Clock::now();
  if(!r->program&&!initialize(r)){g_set_error_literal(error,g_quark_from_static_string("domowik-3d"),1,r->error.c_str());return FALSE;}
  if(r->allocated_width!=r->width||r->allocated_height!=r->height){
    if(!r->texture)glGenTextures(1,&r->texture);glBindTexture(GL_TEXTURE_2D,r->texture);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MIN_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_MAG_FILTER,GL_LINEAR);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_S,GL_CLAMP_TO_EDGE);glTexParameteri(GL_TEXTURE_2D,GL_TEXTURE_WRAP_T,GL_CLAMP_TO_EDGE);glTexImage2D(GL_TEXTURE_2D,0,GL_RGBA8,r->width,r->height,0,GL_RGBA,GL_UNSIGNED_BYTE,nullptr);
    if(!r->framebuffer)glGenFramebuffers(1,&r->framebuffer);glBindFramebuffer(GL_FRAMEBUFFER,r->framebuffer);glFramebufferTexture2D(GL_FRAMEBUFFER,GL_COLOR_ATTACHMENT0,GL_TEXTURE_2D,r->texture,0);
    if(!r->depth)glGenRenderbuffers(1,&r->depth);glBindRenderbuffer(GL_RENDERBUFFER,r->depth);glRenderbufferStorage(GL_RENDERBUFFER,GL_DEPTH_COMPONENT24,r->width,r->height);glFramebufferRenderbuffer(GL_FRAMEBUFFER,GL_DEPTH_ATTACHMENT,GL_RENDERBUFFER,r->depth);
    if(glCheckFramebufferStatus(GL_FRAMEBUFFER)!=GL_FRAMEBUFFER_COMPLETE){r->error="Incomplete 3D framebuffer";g_set_error_literal(error,g_quark_from_static_string("domowik-3d"),2,r->error.c_str());return FALSE;}
    r->allocated_width=r->width;r->allocated_height=r->height;
  }
  glBindFramebuffer(GL_FRAMEBUFFER,r->framebuffer);glViewport(0,0,r->width,r->height);glDisable(GL_SCISSOR_TEST);glDisable(GL_STENCIL_TEST);glColorMask(GL_TRUE,GL_TRUE,GL_TRUE,GL_TRUE);glDisable(GL_CULL_FACE);glEnable(GL_DEPTH_TEST);glDepthFunc(GL_LEQUAL);glDepthMask(GL_TRUE);glClearDepthf(1.f);glDepthRangef(0.f,1.f);glDisable(GL_BLEND);glClearColor(0.84f,0.9f,0.94f,1.f);glClear(GL_COLOR_BUFFER_BIT|GL_DEPTH_BUFFER_BIT);
  upload(r);glUseProgram(r->program);glUniformMatrix4fv(glGetUniformLocation(r->program,"uCamera"),1,GL_FALSE,camera.data());glUniform1i(glGetUniformLocation(r->program,"uBaseColor"),0);
  r->draw_calls=0;r->triangles=0;
  for(bool transparent:{false,true}) {
    if(transparent){glEnable(GL_BLEND);glBlendFuncSeparate(GL_SRC_ALPHA,GL_ONE_MINUS_SRC_ALPHA,GL_ONE,GL_ONE_MINUS_SRC_ALPHA);glDepthMask(GL_FALSE);}
    for(auto& g:r->groups){if(g.transparent!=transparent)continue;auto m=r->meshes.find(g.mesh);if(m==r->meshes.end()||!g.vao)continue;
      glBindTexture(GL_TEXTURE_2D_ARRAY,m->second.image);glBindVertexArray(g.vao);glDrawArraysInstanced(GL_TRIANGLES,0,m->second.vertices.size()/10,g.instances.size()/29);r->draw_calls++;r->triangles+=m->second.vertices.size()/30*(g.instances.size()/29);
    }
  }
  // Readback exists only for an explicit diagnostic probe, never normal frames.
  if(r->diagnostic_readback){std::vector<uint8_t> pixels(r->width*r->height*4);glReadPixels(0,0,r->width,r->height,GL_RGBA,GL_UNSIGNED_BYTE,pixels.data());r->diagnostic_pixels=0;r->orientation_top_red=0;r->orientation_bottom_blue=0;for(size_t i=0;i<pixels.size();i+=4){if(std::abs(int(pixels[i])-214)+std::abs(int(pixels[i+1])-230)+std::abs(int(pixels[i+2])-240)>20)r->diagnostic_pixels++;const size_t row=i/4/r->width;const bool red=pixels[i]>80&&pixels[i]>2*pixels[i+2]&&pixels[i]>2*pixels[i+1];const bool blue=pixels[i+2]>80&&pixels[i+2]>2*pixels[i]&&pixels[i+2]>2*pixels[i+1];if(row<static_cast<size_t>(r->height/2)&&red)r->orientation_top_red++;if(row>=static_cast<size_t>(r->height/2)&&blue)r->orientation_bottom_blue++;}r->diagnostic_readback=false;}
  r->frames++;r->render_us=std::chrono::duration<double,std::micro>(Clock::now()-start).count();
  *target=GL_TEXTURE_2D;*name=r->texture;*width=r->width;*height=r->height;return TRUE;
}

static void dispose_texture(GObject* object){auto* texture=reinterpret_cast<DomowikSceneTexture*>(object);if(texture->renderer){std::lock_guard<std::mutex> lock(retired_mutex);retired.push_back(texture->renderer);texture->renderer=nullptr;}G_OBJECT_CLASS(domowik_scene_texture_parent_class)->dispose(object);}
static void domowik_scene_texture_class_init(DomowikSceneTextureClass* klass){FL_TEXTURE_GL_CLASS(klass)->populate=populate;G_OBJECT_CLASS(klass)->dispose=dispose_texture;}
static void domowik_scene_texture_init(DomowikSceneTexture* texture){texture->renderer=new Renderer();}

namespace {
struct Channel {FlTextureRegistrar* registrar;DomowikSceneTexture* texture=nullptr;uint64_t session=0;};
std::atomic<uint64_t> next_session{1};
FlValue* field(FlValue* value,const char* name){return value&&fl_value_get_type(value)==FL_VALUE_TYPE_MAP?fl_value_lookup_string(value,name):nullptr;}
bool floats(FlValue* value,size_t expected,std::vector<float>& out){
  if(!value)return false;const auto type=fl_value_get_type(value);if(type!=FL_VALUE_TYPE_FLOAT32_LIST&&type!=FL_VALUE_TYPE_FLOAT_LIST)return false;const size_t count=fl_value_get_length(value);
  if(expected&&count!=expected)return false;if(count>10'000'000)return false;
  if(type==FL_VALUE_TYPE_FLOAT32_LIST){const float* p=fl_value_get_float32_list(value);out.assign(p,p+count);}
  else if(type==FL_VALUE_TYPE_FLOAT_LIST){const double* p=fl_value_get_float_list(value);out.assign(p,p+count);}
  else return false;
  return std::all_of(out.begin(),out.end(),[](float v){return std::isfinite(v);});
}
void success(FlMethodCall* call,FlValue* value=nullptr){fl_method_call_respond_success(call,value,nullptr);}
void reject(FlMethodCall* call,const char* message){fl_method_call_respond_error(call,"3d-renderer",message,nullptr,nullptr);}
void notify(Channel* c){fl_texture_registrar_mark_texture_frame_available(c->registrar,FL_TEXTURE(c->texture));}
void method(FlMethodChannel*,FlMethodCall* call,gpointer data){
  auto* c=static_cast<Channel*>(data);const char* name=fl_method_call_get_name(call);FlValue* args=fl_method_call_get_args(call);
  if(g_strcmp0(name,"create")==0){
    if(!c->texture){c->texture=reinterpret_cast<DomowikSceneTexture*>(g_object_new(domowik_scene_texture_get_type(),nullptr));c->session=next_session.fetch_add(1);if(!fl_texture_registrar_register_texture(c->registrar,FL_TEXTURE(c->texture))){g_clear_object(&c->texture);reject(call,"Could not register GPU texture");return;}}
    g_autoptr(FlValue) result=fl_value_new_map();fl_value_set_string_take(result,"textureId",fl_value_new_int(fl_texture_get_id(FL_TEXTURE(c->texture))));fl_value_set_string_take(result,"session",fl_value_new_int(c->session));success(call,result);return;
  }
  FlValue* session=field(args,"session");if(!c->texture||!session||fl_value_get_type(session)!=FL_VALUE_TYPE_INT||static_cast<uint64_t>(fl_value_get_int(session))!=c->session){reject(call,"3D surface is closed or stale");return;}
  if(g_strcmp0(name,"destroy")==0){fl_texture_registrar_unregister_texture(c->registrar,FL_TEXTURE(c->texture));g_clear_object(&c->texture);success(call);return;}
  auto* r=c->texture->renderer;
  if(g_strcmp0(name,"camera")==0){std::vector<float> m;if(!floats(field(args,"matrix"),16,m)){reject(call,"Invalid camera matrix");return;}FlValue* w=field(args,"width");FlValue* h=field(args,"height");if(!w||!h||fl_value_get_type(w)!=FL_VALUE_TYPE_INT||fl_value_get_type(h)!=FL_VALUE_TYPE_INT){reject(call,"Invalid viewport");return;}{std::lock_guard<std::mutex> camera_lock(r->camera_mutex);r->requested_width=std::clamp<int64_t>(fl_value_get_int(w),32,4096);r->requested_height=std::clamp<int64_t>(fl_value_get_int(h),32,4096);std::copy(m.begin(),m.end(),r->camera.begin());r->camera_updates++;}notify(c);success(call);return;}
  std::lock_guard<std::mutex> lock(r->mutex);
  if(g_strcmp0(name,"orientationProbe")==0){
    // Original two-colour texture, world +Y red and -Y blue. White vertex and
    // instance colours mean ignoring UVs/images cannot pass this real-GL probe.
    auto* old=new Renderer();old->context=r->context;old->groups=std::move(r->groups);old->meshes=std::move(r->meshes);{std::lock_guard<std::mutex> retired_lock(retired_mutex);retired.push_back(old);}
    std::vector<float> vertices;
    std::vector<float> coordinates;
    for(auto p:{std::array<float,3>{-0.8f,-0.8f,0.f},{0.8f,-0.8f,0.f},{0.8f,0.8f,0.f},{-0.8f,-0.8f,0.f},{0.8f,0.8f,0.f},{-0.8f,0.8f,0.f}}){vertices.insert(vertices.end(),p.begin(),p.end());vertices.insert(vertices.end(),{0.f,0.f,1.f,1.f,1.f,1.f,1.f});coordinates.insert(coordinates.end(),{0.5f,0.5f-p[1]/1.6f,0.f,1.f,1.f});}
    Mesh fixture{std::move(vertices),0};fixture.coordinates=std::move(coordinates);fixture.width=1;fixture.height=2;fixture.rgba={255,0,0,255,0,0,255,255};
    r->meshes.emplace("diagnostic:orientation",std::move(fixture));
    std::vector<float> instances;
    instances={1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,1.f,0.f,0.f,0.f,1.f,0.f,0.f,0.f,1.f,1.f,1.f,1.f,1.f};
    r->groups.push_back(Group{"diagnostic:orientation",false,std::move(instances),0,0});r->scene_dirty=true;
    {std::lock_guard<std::mutex> camera_lock(r->camera_mutex);r->camera={1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f,0.f,0.f,0.f,0.f,1.f};}
    r->diagnostic_readback=true;notify(c);success(call);return;
  }
  if(g_strcmp0(name,"reset")==0){auto* old=new Renderer();old->context=r->context;old->meshes=std::move(r->meshes);old->groups=std::move(r->groups);{std::lock_guard<std::mutex> retired_lock(retired_mutex);retired.push_back(old);}r->meshes.clear();r->groups.clear();r->scene_dirty=true;notify(c);success(call);return;}
  if(g_strcmp0(name,"mesh")==0){
    FlValue* id=field(args,"id");Mesh incoming;
    if(!id||fl_value_get_type(id)!=FL_VALUE_TYPE_STRING||!floats(field(args,"vertices"),0,incoming.vertices)||incoming.vertices.size()%30){reject(call,"Invalid triangle vertex buffer");return;}
    FlValue* t=field(args,"texture");
    if(t){
      auto dimension=[&](const char* key,int max){FlValue* v=field(t,key);return v&&fl_value_get_type(v)==FL_VALUE_TYPE_INT&&fl_value_get_int(v)>0&&fl_value_get_int(v)<=max?static_cast<int>(fl_value_get_int(v)):0;};
      incoming.width=dimension("width",2048);incoming.height=dimension("height",2048);incoming.layers=dimension("layers",16);
      const uint64_t size=uint64_t(incoming.width)*incoming.height*incoming.layers*4;FlValue* rgba=field(t,"rgba");
      if(!incoming.width||!incoming.height||!incoming.layers||size>64*1024*1024||!rgba||fl_value_get_type(rgba)!=FL_VALUE_TYPE_UINT8_LIST||fl_value_get_length(rgba)!=size||!floats(field(t,"coordinates"),incoming.vertices.size()/10*5,incoming.coordinates)||incoming.coordinates.size()!=incoming.vertices.size()/10*5){reject(call,"Invalid base-colour texture");return;}
      for(size_t i=0;i<incoming.coordinates.size();i+=5){const float* uv=&incoming.coordinates[i];if(std::abs(uv[0])>1e6f||std::abs(uv[1])>1e6f||uv[2]<-1||uv[2]>=incoming.layers||std::floor(uv[2])!=uv[2]||uv[3]<0||uv[3]>2||std::floor(uv[3])!=uv[3]||uv[4]<0||uv[4]>2||std::floor(uv[4])!=uv[4]){reject(call,"Invalid UV coordinates/layer/wrap");return;}}
      const auto* bytes=fl_value_get_uint8_list(rgba);incoming.rgba.assign(bytes,bytes+size);
    }
    std::string key=fl_value_get_string(id);auto existing=r->meshes.find(key);
    uint64_t total=incoming.rgba.size();for(const auto& [k,m]:r->meshes)if(k!=key)total+=m.rgba.size();if(total>128*1024*1024){reject(call,"Scene textures exceed 128 MiB");return;}
    if(existing==r->meshes.end()){r->meshes.emplace(key,std::move(incoming));r->scene_dirty=true;}
    else if(existing->second.vertices!=incoming.vertices||existing->second.coordinates!=incoming.coordinates||existing->second.rgba!=incoming.rgba||existing->second.width!=incoming.width||existing->second.height!=incoming.height||existing->second.layers!=incoming.layers){auto* old=new Renderer();old->context=r->context;old->meshes.emplace(key,std::move(existing->second));{std::lock_guard<std::mutex> retired_lock(retired_mutex);retired.push_back(old);}existing->second=std::move(incoming);r->scene_dirty=true;}
    notify(c);success(call);return;
  }
  if(g_strcmp0(name,"scene")==0){FlValue* nodes=field(args,"nodes");if(!nodes||fl_value_get_type(nodes)!=FL_VALUE_TYPE_LIST||fl_value_get_length(nodes)>20000){reject(call,"Invalid scene instances");return;}
    std::map<std::pair<std::string,bool>,std::vector<float>> grouped;
    for(size_t i=0;i<fl_value_get_length(nodes);i++){FlValue* node=fl_value_get_list_value(nodes,i);FlValue* id=field(node,"mesh");FlValue* transparent=field(node,"transparent");std::vector<float> matrix,normal,color;if(!id||fl_value_get_type(id)!=FL_VALUE_TYPE_STRING||!floats(field(node,"matrix"),16,matrix)||!floats(field(node,"normalMatrix"),9,normal)||!floats(field(node,"color"),4,color)){reject(call,"Invalid instance transform");return;}
      auto& group=grouped[{fl_value_get_string(id),transparent&&fl_value_get_type(transparent)==FL_VALUE_TYPE_BOOL&&fl_value_get_bool(transparent)}];group.insert(group.end(),matrix.begin(),matrix.end());group.insert(group.end(),normal.begin(),normal.end());group.insert(group.end(),color.begin(),color.end());
    }
    // Old instance objects are retired on the GL thread, not deleted here.
    auto* old=new Renderer();old->context=r->context;old->groups=std::move(r->groups);{std::lock_guard<std::mutex> retired_lock(retired_mutex);retired.push_back(old);}
    for(auto& [key,instances]:grouped)r->groups.push_back(Group{key.first,key.second,std::move(instances),0,0});r->scene_dirty=true;r->scene_updates++;notify(c);success(call);return;
  }
  if(g_strcmp0(name,"stats")==0){g_autoptr(FlValue) stats=fl_value_new_map();for(auto p:{std::pair<const char*,uint64_t>{"frames",r->frames},{"meshUploads",r->mesh_uploads},{"instanceUploads",r->instance_uploads},{"cameraUpdates",r->camera_updates},{"sceneUpdates",r->scene_updates},{"triangles",r->triangles},{"drawCalls",r->draw_calls},{"diagnosticPixels",r->diagnostic_pixels}})fl_value_set_string_take(stats,p.first,fl_value_new_int(p.second));fl_value_set_string_take(stats,"renderSubmitUs",fl_value_new_float(r->render_us));fl_value_set_string_take(stats,"driver",fl_value_new_string(r->driver.c_str()));fl_value_set_string_take(stats,"error",fl_value_new_string(r->error.c_str()));success(call,stats);return;}
  if(g_strcmp0(name,"diagnosticCapture")==0){r->diagnostic_readback=true;notify(c);success(call);return;}
  if(g_strcmp0(name,"orientationStats")==0){g_autoptr(FlValue) result=fl_value_new_map();fl_value_set_string_take(result,"topRedPixels",fl_value_new_int(r->orientation_top_red));fl_value_set_string_take(result,"bottomBluePixels",fl_value_new_int(r->orientation_bottom_blue));success(call,result);return;}
  fl_method_call_respond_not_implemented(call,nullptr);
}
void release_channel(gpointer data){auto* c=static_cast<Channel*>(data);if(c->texture){fl_texture_registrar_unregister_texture(c->registrar,FL_TEXTURE(c->texture));g_clear_object(&c->texture);}g_object_unref(c->registrar);delete c;}
}

void register_scene_texture(FlView* view){g_autoptr(FlStandardMethodCodec) codec=fl_standard_method_codec_new();g_autoptr(FlMethodChannel) channel=fl_method_channel_new(fl_engine_get_binary_messenger(fl_view_get_engine(view)),"invalid.domowik-spike/three-d",FL_METHOD_CODEC(codec));auto* data=new Channel{FL_TEXTURE_REGISTRAR(g_object_ref(fl_engine_get_texture_registrar(fl_view_get_engine(view))))};fl_method_channel_set_method_call_handler(channel,method,data,release_channel);}
