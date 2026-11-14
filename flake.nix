{
  description = "parentcontrol: shared time limits for selected programs on home PCs";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
    }:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          let
            pkgs = import nixpkgs {
              inherit system;
              overlays = [ rust-overlay.overlays.default ];
            };
            # Same toolchain, components and targets as rustup would install.
            rust = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          in
          f { inherit pkgs rust; }
        );
    in
    {
      packages = forAllSystems (
        { pkgs, rust }:
        {
          default = pkgs.callPackage ./nix/package.nix {
            rustPlatform = pkgs.makeRustPlatform {
              cargo = rust;
              rustc = rust;
            };
          };
        }
      );

      nixosModules.default = import ./nix/module.nix { inherit self; };

      # A VM test of the NixOS module; `nix flake check` runs it.
      checks = forAllSystems (
        { pkgs, ... }:
        {
          agent-vm = import ./nix/test.nix { inherit self pkgs; };
        }
      );

      devShells = forAllSystems (
        { pkgs, rust }:
        let
          mingw = pkgs.pkgsCross.mingwW64;
          musl = pkgs.pkgsCross.musl64;
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.just
              pkgs.cargo-deny
            ];

            # Cross-linking for Windows. The linker is referenced directly
            # instead of being put on PATH so its setup hooks do not leak
            # Windows flags into host builds.
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${mingw.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
            # C parts of dependencies (ring) compiled by the `cc` crate.
            CC_x86_64_pc_windows_gnu = "${mingw.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
            AR_x86_64_pc_windows_gnu = "${mingw.stdenv.cc.bintools.bintools}/bin/x86_64-w64-mingw32-ar";
            CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${mingw.windows.pthreads}/lib";
            # Static Linux binaries: C parts of dependencies need a musl compiler.
            CC_x86_64_unknown_linux_musl = "${musl.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc";
            CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${musl.stdenv.cc}/bin/x86_64-unknown-linux-musl-gcc";

            # rustc calls `x86_64-w64-mingw32-dlltool` by name for import
            # libraries; the unwrapped binutils carry no setup hooks.
            shellHook = ''
              export PATH="${mingw.stdenv.cc.bintools.bintools}/bin:$PATH"
            '';
          };
        }
      );

      formatter = forAllSystems ({ pkgs, ... }: pkgs.nixfmt);
    };
}
