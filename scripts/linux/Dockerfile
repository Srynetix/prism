# The image scripts/linux.sh builds Prism's Linux bundles in: Ubuntu 22.04, so that they run on
# any distribution as recent (glibc 2.35, WebKitGTK 4.1), with Rust, Node and pnpm.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
# Tauri's Linux prerequisites, the keyring's D-Bus, what bundling an AppImage runs, and the
# dictation's: ALSA (the microphone), cmake and libclang (whisper.cpp, built with the app, and its bindgen).
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential ca-certificates curl wget file git rsync pkg-config xz-utils \
      libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev libdbus-1-dev \
      libasound2-dev cmake libclang-dev \
    && rm -rf /var/lib/apt/lists/*

ARG NODE_VERSION=24.11.1
RUN arch=$(dpkg --print-architecture | sed 's/amd64/x64/') \
    && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${arch}.tar.xz" | tar -xJ -C /usr/local --strip-components=1 \
    && npm install -g pnpm@10.11.1

ENV RUSTUP_HOME=/usr/local/rustup PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | CARGO_HOME=/usr/local/cargo sh -s -- -y --profile minimal --default-toolchain stable

# The build copies the sources (git included: the app names its commit), owned by someone else.
RUN git config --system --add safe.directory '*'
