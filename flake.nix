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
						"aarch64-apple-darwin"
					];
				};
				rustPlatform = pkgs.makeRustPlatform {
					cargo = rustToolchain;
					rustc = rustToolchain;
				};
				cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
			in
			{
				packages.default = rustPlatform.buildRustPackage {
					pname = cargoToml.package.name;
					version = cargoToml.package.version;
					src = ./.;
					cargoLock.lockFile = ./Cargo.lock;
				};

				devShells.default = pkgs.mkShell ({
					buildInputs = with pkgs; [
						rustToolchain
						rust-analyzer
						cargo-make
						zig
						cargo-zigbuild
						cargo-llvm-cov
					] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
						pkgs.pkgsCross.musl64.stdenv.cc
						pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc
					];

					RUST_SRC_PATH = "${rustToolchain}/lib/rustlib/src/rust/library";
				} // pkgs.lib.optionalAttrs pkgs.stdenv.isLinux {
					CC_x86_64_unknown_linux_musl = "${pkgs.pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";
					CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsCross.musl64.stdenv.cc}/bin/x86_64-unknown-linux-musl-cc";
					CC_aarch64_unknown_linux_musl = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc}/bin/aarch64-unknown-linux-musl-cc";
					CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER = "${pkgs.pkgsCross.aarch64-multiplatform-musl.stdenv.cc}/bin/aarch64-unknown-linux-musl-cc";
				});
			}
		);
}
