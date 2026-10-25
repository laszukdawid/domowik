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
  return g_test_run();
}
