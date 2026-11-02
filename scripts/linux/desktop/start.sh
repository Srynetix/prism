#!/usr/bin/env bash
# Runs in scripts/linux/desktop/Dockerfile's image: an Xfce desktop on VNC :1 (port 5901), noVNC
# on 6080, Prism started in it. VNC_PASSWORD, drawn per run by scripts/linux-desktop.sh, which
# publishes the ports on 127.0.0.1 only: a web page the host's browser opens can still reach
# localhost (websockify checks no origin), not without it.
set -euo pipefail

mkdir -p ~/.vnc ~/.config/autostart
cat > ~/.vnc/xstartup <<'SH'
#!/bin/sh
unset SESSION_MANAGER
exec dbus-launch --exit-with-session startxfce4
SH
chmod +x ~/.vnc/xstartup
[[ -n "${VNC_PASSWORD:-}" ]] || { echo "VNC_PASSWORD is not set" >&2; exit 1; }
printf '%s\n' "$VNC_PASSWORD" | vncpasswd -f > ~/.vnc/passwd
chmod 600 ~/.vnc/passwd
cat > ~/.config/autostart/prism.desktop <<'INI'
[Desktop Entry]
Type=Application
Name=Prism
Exec=sh -c 'prism > /tmp/prism.log 2>&1'
INI

vncserver :1 -geometry "${GEOMETRY:-1600x1000}" -depth 24 -localhost no \
  -SecurityTypes VncAuth -PasswordFile ~/.vnc/passwd
websockify --web /usr/share/novnc 6080 localhost:5901 &
echo "ready"
tail -F ~/.vnc/*.log /tmp/prism.log 2>/dev/null
