{
	description = "Rust development environment";

	inputs = {
		nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
		flake-utils.url = "github:numtide/flake-utils";
		rust-overlay = {
			url = "github:oxalica/rust-overlay";
			inputs.nixpkgs.follows = "nixpkgs";
		};
	};

	outputs = { self, nixpkgs, flake-utils, rust-overlay }:
		flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ] (system:
			let
				overlays = [ (import rust-overlay) ];
				pkgs = import nixpkgs {
					inherit system overlays;
				};
				rustToolchain = pkgs.rust-bin.nightly.latest.default.override {
					extensions = [ "llvm-tools-preview" "rust-src" ];
					targets = [
						"x86_64-unknown-linux-musl"
						"aarch64-unknown-linux-musl"
						"riscv64gc-unknown-linux-musl"
						"x86_64-apple-darwin"
						"aarch64-apple-darwin"
						"x86_64-pc-windows-gnullvm"
						"aarch64-pc-windows-gnullvm"
					];
				};
				# Minimal nightly toolchain for CI: just rustc + cargo for the host target.
				rustToolchainCI = pkgs.rust-bin.nightly.latest.minimal;
				# Nix package uses the same minimal toolchain which is all that is needed to build for the current
				# system.
				rustPlatform = pkgs.makeRustPlatform {
					cargo = rustToolchainCI;
					rustc = rustToolchainCI;
				};
				cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
			in
			{
				packages.default = rustPlatform.buildRustPackage {
					pname = cargoToml.package.name;
					version = cargoToml.package.version;
					src = ./.;
					cargoLock.lockFile = ./Cargo.lock;

					# Skip nix-shell integration tests: they require `nix develop` which
					# is not available inside the Nix sandbox.
					cargoTestFlags = [ "--no-default-features" ];

					# Required for managing the node module in this project
					nativeBuildInputs = with pkgs; [
						git
						nodejs
					];
				};

				devShells.default = pkgs.mkShell {
					buildInputs = with pkgs; [
						# Rust toolchain
						rustToolchain
						rust-analyzer
						cargo-deny
						cargo-insta
						cargo-make
						cargo-mutants
						cargo-llvm-cov

						# Cross-compilation tools
						zig
						cargo-zigbuild

						# Lint tools
						markdownlint-cli

						# Required for managing the node module in this project
						nodejs
					];

					RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
				};

				# Minimal shell for CI: only what is needed for `cargo build --release`
				# and `cursus ci` on the current host architecture.
				devShells.ci = pkgs.mkShell {
					buildInputs = with pkgs; [
						rustToolchainCI
						git
						nodejs
					];
				};

				# Minimal shells for package-manager-specific integration tests.
				# Each shell provides exactly the tools available in a typical user
				# installation of that package manager. Tests use these via
				# `run_cursus_in_nix_shell` to exercise auto-detection in isolation.
				devShells.test-npm = pkgs.mkShell {
					buildInputs = with pkgs; [ git nodejs ];
				};

				devShells.test-pnpm = pkgs.mkShell {
					buildInputs = with pkgs; [ git nodejs nodePackages.pnpm ];
				};

				devShells.test-yarn-classic = pkgs.mkShell {
					buildInputs = with pkgs; [ git nodejs nodePackages.yarn ];
				};

				# yarn-berry provides the `yarn` binary directly — no wrapper needed.
				devShells.test-yarn-berry = pkgs.mkShell {
					buildInputs = with pkgs; [ git nodejs yarn-berry ];
				};
			}
		);
}
