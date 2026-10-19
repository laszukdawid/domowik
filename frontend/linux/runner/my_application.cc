#include "my_application.h"
#include "file_chooser.h"
#include "scene_texture.h"

#include <flutter_linux/flutter_linux.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#include "flutter/generated_plugin_registrant.h"

struct _MyApplication {
  GtkApplication parent_instance;
  char** dart_entrypoint_arguments;
};

G_DEFINE_TYPE(MyApplication, my_application, GTK_TYPE_APPLICATION)

// Platform UI stays on GTK's main thread. The chooser retains the method call
// until its async response; cancellation returns null, never an invented path.
static void file_chooser_response(GtkNativeDialog* dialog, gint response,
                                     gpointer user_data) {
  FlMethodCall* call = FL_METHOD_CALL(
      g_object_get_data(G_OBJECT(dialog), "file-chooser-call"));
  g_autofree gchar* path = response == GTK_RESPONSE_ACCEPT
      ? gtk_file_chooser_get_filename(GTK_FILE_CHOOSER(dialog)) : nullptr;
  g_autoptr(FlValue) result = path != nullptr
      ? fl_value_new_string(path) : fl_value_new_null();
  fl_method_call_respond_success(call, result, nullptr);
  gtk_native_dialog_destroy(dialog);
  g_object_unref(dialog);
}

static void file_method(FlMethodChannel* channel, FlMethodCall* call,
                                   gpointer user_data) {
  const gchar* method = fl_method_call_get_name(call);
  if (g_strcmp0(method, "pickAssetDirectory") != 0 &&
      g_strcmp0(method, "openPlan") != 0 &&
      g_strcmp0(method, "savePlan") != 0) {
    fl_method_call_respond_not_implemented(call, nullptr);
    return;
  }
  g_autoptr(GObject) parent = G_OBJECT(g_weak_ref_get(static_cast<GWeakRef*>(user_data)));
  if (parent == nullptr) {
    fl_method_call_respond_error(call, "window-closed", "The editor window is closed", nullptr, nullptr);
    return;
  }
  FlValue* initial = fl_method_call_get_args(call);
  GtkFileChooserNative* chooser = create_file_chooser(
      GTK_WINDOW(parent), method,
      initial != nullptr && fl_value_get_type(initial) == FL_VALUE_TYPE_STRING
          ? fl_value_get_string(initial) : nullptr);
  g_object_set_data_full(G_OBJECT(chooser), "file-chooser-call", g_object_ref(call), g_object_unref);
  g_signal_connect(chooser, "response", G_CALLBACK(file_chooser_response), nullptr);
  gtk_native_dialog_show(GTK_NATIVE_DIALOG(chooser));
}

static void free_window_reference(gpointer data) {
  auto* reference = static_cast<GWeakRef*>(data);
  g_weak_ref_clear(reference);
  g_free(reference);
}

// Called when first Flutter frame received.
static void first_frame_cb(MyApplication* self, FlView* view) {
  gtk_widget_show(gtk_widget_get_toplevel(GTK_WIDGET(view)));
}

// Implements GApplication::activate.
static void my_application_activate(GApplication* application) {
  MyApplication* self = MY_APPLICATION(application);
  GtkWindow* window =
      GTK_WINDOW(gtk_application_window_new(GTK_APPLICATION(application)));

  // Use a header bar when running in GNOME as this is the common style used
  // by applications and is the setup most users will be using (e.g. Ubuntu
  // desktop).
  // If running on X and not using GNOME then just use a traditional title bar
  // in case the window manager does more exotic layout, e.g. tiling.
  // If running on Wayland assume the header bar will work (may need changing
  // if future cases occur).
  gboolean use_header_bar = TRUE;
#ifdef GDK_WINDOWING_X11
  GdkScreen* screen = gtk_window_get_screen(window);
  if (GDK_IS_X11_SCREEN(screen)) {
    const gchar* wm_name = gdk_x11_screen_get_window_manager_name(screen);
    if (g_strcmp0(wm_name, "GNOME Shell") != 0) {
      use_header_bar = FALSE;
    }
  }
#endif
  if (use_header_bar) {
    GtkHeaderBar* header_bar = GTK_HEADER_BAR(gtk_header_bar_new());
    gtk_widget_show(GTK_WIDGET(header_bar));
    gtk_header_bar_set_title(header_bar, "Domowik");
    gtk_header_bar_set_show_close_button(header_bar, TRUE);
    gtk_window_set_titlebar(window, GTK_WIDGET(header_bar));
  } else {
    gtk_window_set_title(window, "Domowik");
  }

  gtk_window_set_default_size(window, 1280, 720);

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(
      project, self->dart_entrypoint_arguments);

  FlView* view = fl_view_new(project);
  GdkRGBA background_color;
  // Background defaults to black, override it here if necessary, e.g. #00000000
  // for transparent.
  gdk_rgba_parse(&background_color, "#000000");
  fl_view_set_background_color(view, &background_color);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));

  // Show the window when Flutter renders.
  // Requires the view to be realized so we can start rendering.
  g_signal_connect_swapped(view, "first-frame", G_CALLBACK(first_frame_cb),
                           self);
  gtk_widget_realize(GTK_WIDGET(view));

  fl_register_plugins(FL_PLUGIN_REGISTRY(view));
  register_scene_texture(view);

  g_autoptr(FlStandardMethodCodec) codec = fl_standard_method_codec_new();
  g_autoptr(FlMethodChannel) files = fl_method_channel_new(
      fl_engine_get_binary_messenger(fl_view_get_engine(view)),
      "invalid.domowik-spike/files", FL_METHOD_CODEC(codec));
  auto* reference = g_new0(GWeakRef, 1);
  g_weak_ref_init(reference, G_OBJECT(window));
  fl_method_channel_set_method_call_handler(files, file_method,
                                          reference, free_window_reference);

  gtk_widget_grab_focus(GTK_WIDGET(view));
}

// Implements GApplication::local_command_line.
static gboolean my_application_local_command_line(GApplication* application,
                                                  gchar*** arguments,
                                                  int* exit_status) {
  MyApplication* self = MY_APPLICATION(application);
  // Strip out the first argument as it is the binary name.
  self->dart_entrypoint_arguments = g_strdupv(*arguments + 1);

  g_autoptr(GError) error = nullptr;
  if (!g_application_register(application, nullptr, &error)) {
    g_warning("Failed to register: %s", error->message);
    *exit_status = 1;
    return TRUE;
  }

  g_application_activate(application);
  *exit_status = 0;

  return TRUE;
}

// Implements GApplication::startup.
static void my_application_startup(GApplication* application) {
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application startup.

  G_APPLICATION_CLASS(my_application_parent_class)->startup(application);
}

// Implements GApplication::shutdown.
static void my_application_shutdown(GApplication* application) {
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application shutdown.

  G_APPLICATION_CLASS(my_application_parent_class)->shutdown(application);
}

// Implements GObject::dispose.
static void my_application_dispose(GObject* object) {
  MyApplication* self = MY_APPLICATION(object);
  g_clear_pointer(&self->dart_entrypoint_arguments, g_strfreev);
  G_OBJECT_CLASS(my_application_parent_class)->dispose(object);
}

static void my_application_class_init(MyApplicationClass* klass) {
  G_APPLICATION_CLASS(klass)->activate = my_application_activate;
  G_APPLICATION_CLASS(klass)->local_command_line =
      my_application_local_command_line;
  G_APPLICATION_CLASS(klass)->startup = my_application_startup;
  G_APPLICATION_CLASS(klass)->shutdown = my_application_shutdown;
  G_OBJECT_CLASS(klass)->dispose = my_application_dispose;
}

static void my_application_init(MyApplication* self) {}

MyApplication* my_application_new() {
  // Set the program name to the application ID, which helps various systems
  // like GTK and desktop environments map this running application to its
  // corresponding .desktop file. This ensures better integration by allowing
  // the application to be recognized beyond its binary name.
  g_set_prgname(APPLICATION_ID);

  return MY_APPLICATION(g_object_new(my_application_get_type(),
                                     "application-id", APPLICATION_ID, "flags",
                                     G_APPLICATION_NON_UNIQUE, nullptr));
}
