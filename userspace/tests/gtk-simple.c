/*
 * gtk-simple.c — Minimal GTK3 window WITHOUT GApplication/DBus
 *
 * — NeonVale: Uses gtk_init + gtk_window_new directly.
 * No GApplication, no DBus dependency. Just a window.
 */

#include <gtk/gtk.h>

int main(int argc, char *argv[]) {
    gtk_init(&argc, &argv);

    GtkWidget *window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(window), "OXIDE OS");
    gtk_window_set_default_size(GTK_WINDOW(window), 400, 300);
    g_signal_connect(window, "destroy", G_CALLBACK(gtk_main_quit), NULL);

    GtkWidget *label = gtk_label_new("OXIDE OS - GTK3 on Wayland");
    gtk_container_add(GTK_CONTAINER(window), label);

    gtk_widget_show_all(window);
    gtk_main();

    return 0;
}
