#include "file_chooser.h"

static void check_chooser(const gchar* method, GtkFileChooserAction action,
                          const gchar* initial = nullptr) {
  auto* chooser = create_file_chooser(nullptr, method, initial);
  g_assert_nonnull(chooser);
  auto* files = GTK_FILE_CHOOSER(chooser);
  g_assert_cmpint(gtk_file_chooser_get_action(files), ==, action);
  g_assert_true(gtk_file_chooser_get_local_only(files));
  g_assert_false(gtk_file_chooser_get_select_multiple(files));
  g_assert_true(gtk_native_dialog_get_modal(GTK_NATIVE_DIALOG(chooser)));
  if (action != GTK_FILE_CHOOSER_ACTION_SELECT_FOLDER) {
    auto* filter = gtk_file_chooser_get_filter(files);
    GtkFileFilterInfo info = {};
    info.contains = static_cast<GtkFileFilterFlags>(
        GTK_FILE_FILTER_FILENAME | GTK_FILE_FILTER_DISPLAY_NAME);
    info.filename = "/tmp/house.domo";
    info.display_name = "house.domo";
    g_assert_true(gtk_file_filter_filter(filter, &info));
    info.filename = "/tmp/house.txt";
    info.display_name = "house.txt";
    g_assert_false(gtk_file_filter_filter(filter, &info));
  }
  if (action == GTK_FILE_CHOOSER_ACTION_SAVE) {
    g_assert_true(gtk_file_chooser_get_do_overwrite_confirmation(files));
    g_autofree gchar* name = gtk_file_chooser_get_current_name(files);
    g_assert_cmpstr(name, ==, initial != nullptr ? initial : "plan.domo");
  }
  gtk_native_dialog_destroy(GTK_NATIVE_DIALOG(chooser));
  g_object_unref(chooser);
}

int main(int argc, char** argv) {
  gtk_init(&argc, &argv);
  g_test_init(&argc, &argv, nullptr);
  g_test_add_func("/file-channel/openPlan", [] {
    check_chooser("openPlan", GTK_FILE_CHOOSER_ACTION_OPEN);
  });
  g_test_add_func("/file-channel/savePlan", [] {
    check_chooser("savePlan", GTK_FILE_CHOOSER_ACTION_SAVE, "Our home.domo");
    check_chooser("savePlan", GTK_FILE_CHOOSER_ACTION_SAVE);
  });
  g_test_add_func("/file-channel/pickAssetDirectory", [] {
    check_chooser("pickAssetDirectory", GTK_FILE_CHOOSER_ACTION_SELECT_FOLDER, "/tmp");
  });
  g_test_add_func("/file-channel/unknown", [] {
    g_assert_null(create_file_chooser(nullptr, "unknown", nullptr));
  });
  return g_test_run();
}
