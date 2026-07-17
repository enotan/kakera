{
  description = "Kakera development environment";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
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

      desktopItem = pkgs.makeDesktopItem {
        name = "kakera";
        desktopName = "Kakera";
        comment = "Visual novel library and launcher";
        exec = "kakera";
        icon = "kakera";
        categories = [
          "Game"
          "Utility"
        ];
      };

      kakeraPackage = pkgs.rustPlatform.buildRustPackage {
        pname = "kakera";
        version = "0.1.4";
        src = pkgs.lib.cleanSource ./.;
        cargoLock.lockFile = ./Cargo.lock;

        nativeBuildInputs = with pkgs; [
          dioxus-cli
          pkg-config
          wrapGAppsHook3
        ];

        buildInputs = nativeLibraries;

        buildPhase = ''
          runHook preBuild
          dx build --release --platform linux
          runHook postBuild
        '';

        installPhase = ''
          runHook preInstall

          install -Dm755 \
            target/dx/kakera/release/linux/app/kakera \
            "$out/bin/kakera"

          mkdir -p "$out/bin/assets"
          cp -r \
            target/dx/kakera/release/linux/app/assets/. \
            "$out/bin/assets/"

          install -Dm644 \
            assets/favicon.png \
            "$out/share/icons/hicolor/256x256/apps/kakera.png"

          mkdir -p "$out/share/applications"
          cp \
            ${desktopItem}/share/applications/kakera.desktop \
            "$out/share/applications/kakera.desktop"
          runHook postInstall
        '';

        meta = {
          description = "Visual novel library, launcher, and playtime tracker";
          homepage = "https://github.com/enotan/kakera";
          license = pkgs.lib.licenses.mit;
          mainProgram = "kakera";
          platforms = pkgs.lib.platforms.linux;
        };

      };
    in
    {
      packages.${system} = {
        kakera = kakeraPackage;
        default = kakeraPackage;
      };
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
