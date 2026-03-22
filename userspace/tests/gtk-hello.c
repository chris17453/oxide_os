/*
 * gtk-hello.c — GTK3 "Hello World" for OXIDE OS
 *
 * — NeonVale: The first GTK window on OXIDE. If this renders,
 * the entire stack works: libc → glib → cairo → pango → gdk → gtk → wayland.
 *
 * Build: oxide-cc $(pkg-config --cflags gtk+-3.0) -o gtk-hello gtk-hello.c \
 *        $(pkg-config --libs gtk+-3.0)
 */

#include <gtk/gtk.h>

static void on_activate(GtkApplication *app, gpointer user_data) {
    GtkWidget *window = gtk_application_window_new(app);
    gtk_window_set_title(GTK_WINDOW(window), "OXIDE OS — GTK3 Works!");
    gtk_window_set_default_size(GTK_WINDOW(window), 400, 300);

    GtkWidget *label = gtk_label_new(NULL);
    gtk_label_set_markup(GTK_LABEL(label),
        "<span size='xx-large' weight='bold'>OXIDE OS</span>\n\n"
        "<span size='large'>GTK3 running on Wayland</span>\n\n"
        "— NeonVale: The neon grid awakens.");
    gtk_label_set_justify(GTK_LABEL(label), GTK_JUSTIFY_CENTER);
    gtk_container_add(GTK_CONTAINER(window), label);

    gtk_widget_show_all(window);
}

int main(int argc, char *argv[]) {
    GtkApplication *app = gtk_application_new("org.oxide.hello",
                                               G_APPLICATION_DEFAULT_FLAGS);
    g_signal_connect(app, "activate", G_CALLBACK(on_activate), NULL);
    int status = g_application_run(G_APPLICATION(app), argc, argv);
    g_object_unref(app);
    return status;
}
