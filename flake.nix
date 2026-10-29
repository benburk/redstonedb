{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";
    # The commit Cargo.toml pins the MCHPRS crates to; keep the two equal.
    mchprs = {
      url = "github:benburk/MCHPRS/f605490b5c414d5c54e927b019892815ea53698e";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      mchprs,
    }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
      # Cargo.toml is the one place the package is named and versioned.
      cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = cargoToml.package.name;
            version = cargoToml.package.version;
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
              # MCHPRS and its NBT fork come from git, pinned in Cargo.lock.
              allowBuiltinFetchGit = true;
            };
            # Queries compile against the metadata in .sqlx/, not a database.
            env.SQLX_OFFLINE = "true";
            # MCHPRS's block macros read its mc_data/ from two directories
            # above each crate, which vendoring leaves out.
            preBuild = ''
              cp -r ${mchprs}/mc_data "$NIX_BUILD_TOP/mc_data"
            '';
            meta = {
              mainProgram = cargoToml.package.name;
            };
          };
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              clippy
              rust-analyzer
              rustfmt
              sqlx-cli
              openssl
              pkg-config
              dprint
              fd
              ripgrep
              nixfmt
              watchexec
              sqlite
              tailwindcss_4
            ];
          };
        }
      );
    };
}
