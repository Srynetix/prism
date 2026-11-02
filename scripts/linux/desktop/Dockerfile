# The image scripts/linux-desktop.sh runs a Linux build of Prism in: Ubuntu 22.04 (the bundles'
# baseline) with an Xfce desktop behind VNC, and noVNC to reach it from a browser.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      xfce4 xfce4-terminal dbus-x11 gnome-keyring tigervnc-standalone-server tigervnc-common tigervnc-tools \
      novnc websockify fonts-dejavu fonts-noto-color-emoji locales sudo ca-certificates git \
    && locale-gen fr_FR.UTF-8 en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

# The .deb pulls in what Prism needs (WebKitGTK, tmux).
COPY prism.deb /tmp/prism.deb
RUN apt-get update && apt-get install -y --no-install-recommends /tmp/prism.deb \
    && rm -rf /var/lib/apt/lists/* /tmp/prism.deb

RUN useradd -m -s /bin/bash -G sudo demo && echo 'demo ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/demo
COPY start.sh /usr/local/bin/start-desktop
COPY demo-config.toml /home/demo/.config/prism/config.toml
RUN chown -R demo:demo /home/demo

USER demo
WORKDIR /home/demo
# No WEBKIT_DISABLE_COMPOSITING_MODE nor WEBKIT_DISABLE_DMABUF_RENDERER: WebKitGTK 2.50 crashes
# with either as soon as a page change composites (AcceleratedBackingStore::update), while it
# draws fine on its own with Mesa's software renderer.
ENV LANG=fr_FR.UTF-8 USER=demo
EXPOSE 5901 6080
CMD ["start-desktop"]
