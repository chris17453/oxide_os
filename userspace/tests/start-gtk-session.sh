#!/bin/sh
# — NeonVale: Start a GTK session on OXIDE OS
# Run this from the shell to launch the Wayland compositor,
# D-Bus daemon, and GTK hello world app.

echo "=== Starting OXIDE GTK Session ==="

# Start D-Bus daemon in background
echo "Starting oxide-dbusd..."
/usr/bin/oxide-dbusd &
DBUS_PID=$!
sleep 1

# Set D-Bus session address
export DBUS_SESSION_BUS_ADDRESS="unix:path=/run/dbus/system_bus_socket"

# Start Wayland compositor in background
echo "Starting oxide-wayland compositor..."
/usr/bin/oxide-wayland &
WAYLAND_PID=$!
sleep 1

# Set Wayland display
export WAYLAND_DISPLAY=wayland-0
export XDG_RUNTIME_DIR=/run

# Launch GTK hello world
echo "Launching GTK hello world..."
/usr/bin/gtk-hello

# Cleanup
kill $WAYLAND_PID 2>/dev/null
kill $DBUS_PID 2>/dev/null
echo "=== GTK Session ended ==="
