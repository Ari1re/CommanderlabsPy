# Toolchain for building Commander Lab from source on NixOS:
#   nix-shell            then ./run-linux.sh (it also enters this shell by itself)
# The desktop app (./run-linux.sh) needs the WebKitGTK libraries below; the
# browser edition (./run-linux.sh --web) needs only Rust, a C compiler and Node.js.
{ pkgs ? import <nixpkgs> { } }:

pkgs.mkShell {
  packages = with pkgs; [
    cargo
    rustc
    gcc
    nodejs_22
    pkg-config
    git

    # Desktop window (Tauri)
    webkitgtk_4_1
    gtk3
    libsoup_3
    glib-networking
    librsvg
    openssl
    gsettings-desktop-schemas
  ];

  shellHook = ''
    export GIO_MODULE_DIR=${pkgs.glib-networking}/lib/gio/modules/
    # GTK settings schemas (file pickers crash without them) and a renderer
    # that doesn't leave the window blank on NVIDIA / some Wayland setups.
    export XDG_DATA_DIRS=${pkgs.gsettings-desktop-schemas}/share/gsettings-schemas/${pkgs.gsettings-desktop-schemas.name}:${pkgs.gtk3}/share/gsettings-schemas/${pkgs.gtk3.name}:$XDG_DATA_DIRS
    export WEBKIT_DISABLE_DMABUF_RENDERER=1
  '';
}
