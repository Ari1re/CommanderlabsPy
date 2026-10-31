# Toolchain for building Commander Lab from source on NixOS:
#   nix-shell            then ./run-linux.sh (it also enters this shell by itself)
# The browser edition needs only Rust, a C compiler and Node.js. The desktop
# window (./run-linux.sh --desktop) also needs the WebKitGTK libraries below.
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
  ];

  shellHook = ''
    export GIO_MODULE_DIR=${pkgs.glib-networking}/lib/gio/modules/
  '';
}
