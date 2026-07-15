{
    description = "Kakera development environment";

    inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    outputs = { nixpkgs, ... }:
        let
            system = "x86_64-linux";
            pkgs = import nixpkgs { inherit system; };

            nativeLibraries = with pkgs; [
                wayland
                wayland-protocols
                libxkbcommon
                webkitgtk_4_1
                gtk3
                glib
                openssl
                xdotool
                libayatana-appindicator
                librsvg
            ];
        in
        {
            devShells.${system}.default = pkgs.mkShell {
                nativeBuildInputs = with pkgs; [
                    cargo
                    rustc
                    rustfmt
                    clippy
                    rust-analyzer
                    dioxus-cli
                    pkg-config
                    clang
                    lld
                    curl
                    wget
                    file
                ];

                buildInputs = nativeLibraries;

                LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath nativeLibraries;

                GDK_BACKEND = "x11,wayland";
            };
        };
}
