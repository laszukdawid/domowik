#ifndef DOMOWIK_FILE_CHOOSER_H_
#define DOMOWIK_FILE_CHOOSER_H_

#include <gtk/gtk.h>

// Returns an owned native chooser, or null for an unknown file-channel method.
// GTK creation and response handling must run on the main thread. Callers
// destroy and unref the chooser after responding to its asynchronous result.
inline GtkFileChooserNative* create_file_chooser(GtkWindow* parent,
                                                const gchar* method,
                                                const gchar* initial) {
  const bool open = g_strcmp0(method, "openPlan") == 0;
  const bool save = g_strcmp0(method, "savePlan") == 0;
  const bool directory = g_strcmp0(method, "pickAssetDirectory") == 0;
  if (!open && !save && !directory) return nullptr;

  auto* chooser = gtk_file_chooser_native_new(
      directory ? "Choose asset directory" : save ? "Save plan" : "Open plan",
      parent,
      directory ? GTK_FILE_CHOOSER_ACTION_SELECT_FOLDER
                : save ? GTK_FILE_CHOOSER_ACTION_SAVE : GTK_FILE_CHOOSER_ACTION_OPEN,
      directory ? "Add directory" : save ? "Save" : "Open", "Cancel");
  auto* files = GTK_FILE_CHOOSER(chooser);
  gtk_file_chooser_set_local_only(files, TRUE);
  gtk_file_chooser_set_select_multiple(files, FALSE);
  gtk_native_dialog_set_modal(GTK_NATIVE_DIALOG(chooser), TRUE);
  if (directory) {
    if (initial != nullptr) gtk_file_chooser_set_current_folder(files, initial);
  } else {
    auto* filter = gtk_file_filter_new();
    gtk_file_filter_set_name(filter, "Domowik plans (*.domo)");
    gtk_file_filter_add_pattern(filter, "*.domo");
    gtk_file_chooser_add_filter(files, filter);
    auto* all = gtk_file_filter_new();
    gtk_file_filter_set_name(all, "All files");
    gtk_file_filter_add_pattern(all, "*");
    gtk_file_chooser_add_filter(files, all);
    gtk_file_chooser_set_filter(files, filter);
    if (save) {
      gtk_file_chooser_set_do_overwrite_confirmation(files, TRUE);
      gtk_file_chooser_set_current_name(files, initial != nullptr ? initial : "plan.domo");
    }
  }
  return chooser;
}

#endif  // DOMOWIK_FILE_CHOOSER_H_
