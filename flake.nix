{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    # Used for dioxus-cli and wasm-bindgen-cli, which must match the dioxus
    # and wasm-bindgen versions pinned in Cargo.toml.
    nixpkgs-unstable.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, nixpkgs-unstable, rust-overlay }:
    let
      # CI runs on x86_64-linux. The others are for contributors. Only the
      # Rust side of the shell is exercised there, and the end-to-end tests
      # are untested outside Linux.
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      devShells = forAllSystems (system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          unstable = import nixpkgs-unstable { inherit system; };
        in {
          default = with pkgs; mkShell {
            packages = [
              (rust-bin.fromRustupToolchainFile ./rust-toolchain.toml)
              (python3.withPackages (ps: [ ps.pytest ps.pytest-playwright ]))
              ruff
              unstable.binaryen
              # ty is still in alpha and moves fast; take it from unstable.
              unstable.ty
              unstable.dioxus-cli
              unstable.wasm-bindgen-cli
            ];
            env = {
              # Chromium only. The full browser set also carries Firefox and
              # WebKit, which nothing here launches, because pytest always
              # passes --browser-channel chromium (see pyproject.toml). That
              # cuts the closure by more than half, which every CI run would
              # otherwise download. `browsers-chromium` leaves out the
              # headless shell too, which is exactly the binary the tests
              # avoid.
              PLAYWRIGHT_BROWSERS_PATH = "${playwright-driver.browsers-chromium}";
              PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
            } // lib.optionalAttrs stdenv.isLinux {
              LD_LIBRARY_PATH = lib.makeLibraryPath [ stdenv.cc.cc ];
            };
          };
        });
    };
}
