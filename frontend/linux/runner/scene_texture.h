#ifndef DOMOWIK_SCENE_TEXTURE_H
#define DOMOWIK_SCENE_TEXTURE_H
#include <flutter_linux/flutter_linux.h>
// Registers the per-engine GPU surface channel. Graphics resources are owned
// by Flutter's current GL context; CPU requests never issue GL commands.
void register_scene_texture(FlView* view);
#endif
