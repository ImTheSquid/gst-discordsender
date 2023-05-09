{
  description = "flake.nix for gstreamer development deps";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;

      libraries = with pkgs; [
        glib.dev
        gst_all_1.gst-plugins-base
        libunwind
        gst_all_1.gstreamer
        gst_all_1.gst-libav
        glib
        gobject-introspection
        cargo
        rustc
      ];

      packages = with pkgs; [
        glib.dev
        gst_all_1.gst-plugins-base
        libunwind
        gst_all_1.gstreamer
        gst_all_1.gst-libav
        pkg-config
        glib
        gobject-introspection
        cargo
        rustc
      ];
    in
    {
      devShell.x86_64-linux = pkgs.mkShell {
        buildInputs = packages;

        shellHook = ''
          export LD_LIBRARY_PATH=${pkgs.lib.makeLibraryPath libraries}:$LD_LIBRARY_PATH
          export GIO_MODULE_DIR=${pkgs.glib-networking}/lib/gio/modules/
        '';
      };
    };
}
