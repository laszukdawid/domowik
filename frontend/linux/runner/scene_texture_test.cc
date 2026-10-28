// Compile separately, never into the application. Exercise the actual packet
// preparation without a GL context or a Flutter engine, using original data.
#include "scene_texture.cc"
#include <limits>

static FlValue* packet() {
  auto* value=fl_value_new_map();
  fl_value_set_string_take(value,"nodes",fl_value_new_list());
  auto* meshes=fl_value_new_map();auto* mesh=fl_value_new_map();
  const float vertices[30]={};
  fl_value_set_string_take(mesh,"vertices",fl_value_new_float32_list(vertices,30));
  fl_value_set_string_take(meshes,"geometry:test",mesh);
  fl_value_set_string_take(value,"meshes",meshes);
  const double camera[16]={1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};
  fl_value_set_string_take(value,"matrix",fl_value_new_float_list(camera,16));
  fl_value_set_string_take(value,"width",fl_value_new_int(800));
  fl_value_set_string_take(value,"height",fl_value_new_int(600));
  return value;
}

int main(int argc,char** argv) {
  g_test_init(&argc,&argv,nullptr);
  g_test_add_func("/scene/linear-light-beige",[]{
    if(!g_getenv("DOMOWIK_TEST_GL")){g_test_skip("Run with DOMOWIK_TEST_GL=1 under Xvfb for the pixel regression");return;}
    g_assert_true(gtk_init_check(nullptr,nullptr));
    auto* window=gtk_window_new(GTK_WINDOW_TOPLEVEL);auto* area=gtk_gl_area_new();
    gtk_gl_area_set_required_version(GTK_GL_AREA(area),3,2);
    gtk_container_add(GTK_CONTAINER(window),area);gtk_widget_show_all(window);
    gtk_gl_area_make_current(GTK_GL_AREA(area));
    g_assert_null(gtk_gl_area_get_error(GTK_GL_AREA(area)));
    auto* texture=reinterpret_cast<DomowikSceneTexture*>(g_object_new(domowik_scene_texture_get_type(),nullptr));
    auto* r=texture->renderer;r->requested_width=r->requested_height=32;
    r->camera={1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1};
    const auto decode=[](double v){return v<=0.04045?v/12.92:std::pow((v+0.055)/1.055,2.4);};
    const auto encode=[](double v){return v<=0.0031308?v*12.92:1.055*std::pow(v,1.0/2.4)-0.055;};
    // Alpha .25 is a glass door; .075 also includes the level ghost factor.
    for(float alpha:{1.f,0.25f,0.075f})for(bool textured:{false,true}){
      clear_gpu(r);r->meshes.clear();r->groups.clear();r->scene_dirty=true;
      std::vector<float> vertices;
      for(auto p:{std::array<float,3>{-0.8f,-0.8f,0.f},{0.8f,-0.8f,0.f},{0.8f,0.8f,0.f},{-0.8f,-0.8f,0.f},{0.8f,0.8f,0.f},{-0.8f,0.8f,0.f}}){
        vertices.insert(vertices.end(),p.begin(),p.end());vertices.insert(vertices.end(),{0,0,-1,1,1,1,1});
      }
      auto mesh=std::make_shared<Mesh>(std::move(vertices));
      if(textured){mesh->rgba={128,128,128,255};for(int i=0;i<6;i++)mesh->coordinates.insert(mesh->coordinates.end(),{0.5f,0.5f,0,0,0});}
      r->meshes.emplace("beige",mesh);
      r->groups.push_back(Group{"beige",alpha<1.f,{1,0,0,0,0,1,0,0,0,0,1,0,0,0,0.5f,1,1,0,0,0,1,0,0,0,1,232.f/255,220.f/255,196.f/255,alpha},0,0});
      uint32_t target,name,width,height;GError* error=nullptr;
      g_assert_true(populate(FL_TEXTURE_GL(texture),&target,&name,&width,&height,&error));g_assert_no_error(error);
      glBindFramebuffer(GL_FRAMEBUFFER,r->framebuffer);uint8_t pixel[4];glReadPixels(16,16,1,1,GL_RGBA,GL_UNSIGNED_BYTE,pixel);
      const double base[3]={232,220,196};
      const double background[3]={0.84,0.9,0.94};
      for(int i=0;i<3;i++){
        const double lit=encode(decode(base[i]/255)*(textured?decode(128./255):1)*0.38);
        const double expected=(lit*alpha+background[i]*(1-alpha))*255;
        g_assert_cmpfloat(std::abs(pixel[i]-expected),<=,2.0);
      }
      g_assert_cmpuint(pixel[3],==,255);
    }
    clear_gpu(r);g_object_unref(texture);
    {std::lock_guard<std::mutex> lock(retired_mutex);for(auto* old:retired)delete old;retired.clear();}
    gtk_widget_destroy(window);
  });
  g_test_add_func("/scene/prepare-outside-render-lock",[]{
    Renderer renderer;
    auto mesh=std::make_shared<Mesh>(std::vector<float>(30,0));
    mesh->buffer=42;renderer.meshes.emplace("geometry:test",mesh);
    const auto snapshot=snapshot_meshes(&renderer);
    g_autoptr(FlValue) args=packet();
    std::lock_guard<std::mutex> render_busy(renderer.mutex);
    PreparedScene prepared;std::string error;
    g_assert_true(prepare_scene(args,snapshot,prepared,error));
    g_assert_true(prepared.meshes.empty()); // compare did not need the lock
    g_assert_cmpuint(mesh->buffer,==,42); // retain unchanged GPU allocation
    g_assert_cmpuint(prepared.camera.size(),==,16);
    g_assert_cmpint(prepared.width,==,800);
  });
  g_test_add_func("/scene/invalid-last-mesh-keeps-visible-scene",[]{
    Renderer renderer;auto mesh=std::make_shared<Mesh>(std::vector<float>(30,1));
    renderer.meshes.emplace("geometry:test",mesh);
    const auto snapshot=snapshot_meshes(&renderer);
    g_autoptr(FlValue) args=packet();
    auto* invalid=fl_value_new_map();const float vertices[30]={std::numeric_limits<float>::infinity()};
    fl_value_set_string_take(invalid,"vertices",fl_value_new_float32_list(vertices,30));
    fl_value_set_string_take(field(args,"meshes"),"geometry:invalid",invalid);
    PreparedScene prepared;std::string error;
    g_assert_false(prepare_scene(args,snapshot,prepared,error));
    g_assert_true(renderer.meshes.at("geometry:test")==mesh);
    g_assert_cmpfloat(mesh->vertices[0],==,1.f);
    g_assert_cmpuint(renderer.mesh_revision,==,snapshot.revision);
  });
  g_test_add_func("/scene/snapshot-owns-payload-after-replacement",[]{
    Renderer renderer;renderer.meshes.emplace("geometry:test",std::make_shared<Mesh>(std::vector<float>(30,1)));
    const auto snapshot=snapshot_meshes(&renderer);
    renderer.meshes.clear();renderer.mesh_revision++;
    g_autoptr(FlValue) args=packet();PreparedScene prepared;std::string error;
    g_assert_true(prepare_scene(args,snapshot,prepared,error));
    g_assert_cmpuint(prepared.meshes.size(),==,1);
    g_assert_cmpfloat(snapshot.meshes.at("geometry:test")->vertices[0],==,1.f);
    g_assert_cmpuint(renderer.mesh_revision,!=,snapshot.revision);
  });
  g_test_add_func("/scene/commit-retires-unreferenced-meshes",[]{
    Renderer renderer;
    for(const char* key:{"model:red","model:blue","geometry:test"})renderer.meshes.emplace(key,std::make_shared<Mesh>(std::vector<float>(30,1)));
    auto blue=renderer.meshes.at("model:blue");
    g_autoptr(FlValue) args=packet();
    const double identity[16]={1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1},normal[9]={1,0,0,0,1,0,0,0,1},color[4]={1,1,1,1};
    for(const char* key:{"model:blue","geometry:test"}){
      auto* node=fl_value_new_map();fl_value_set_string_take(node,"mesh",fl_value_new_string(key));
      fl_value_set_string_take(node,"matrix",fl_value_new_float_list(identity,16));
      fl_value_set_string_take(node,"normalMatrix",fl_value_new_float_list(normal,9));
      fl_value_set_string_take(node,"color",fl_value_new_float_list(color,4));
      fl_value_append_take(field(args,"nodes"),node);
    }
    const auto snapshot=snapshot_meshes(&renderer);PreparedScene prepared;std::string error;
    g_assert_true(prepare_scene(args,snapshot,prepared,error));
    Renderer old;commit_scene(&renderer,&old,prepared);
    // The superseded palette goes to the GL thread; the rest stays resident.
    g_assert_cmpuint(renderer.meshes.size(),==,2);
    g_assert_true(renderer.meshes.at("model:blue")==blue);
    g_assert_cmpuint(old.meshes.count("model:red"),==,1);
    g_assert_cmpuint(renderer.mesh_revision,!=,snapshot.revision);
  });
  return g_test_run();
}
