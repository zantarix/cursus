fn main() {
	emit_workspace_root();
}

/// Emits `CURSUS_WORKSPACE_ROOT` so integration tests can locate `flake.nix`.
///
/// Walks up from `CARGO_MANIFEST_DIR` until it finds a directory containing
/// `flake.nix`, then sets the env var for compile-time access via `env!()`.
fn emit_workspace_root() {
	let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
		Ok(d) => d,
		Err(_) => return,
	};
	let mut dir = std::path::PathBuf::from(&manifest_dir);
	loop {
		if dir.join("flake.nix").exists() {
			println!("cargo:rustc-env=CURSUS_WORKSPACE_ROOT={}", dir.display());
			return;
		}
		if !dir.pop() {
			eprintln!("cargo:warning=Could not find flake.nix above {manifest_dir}");
			break;
		}
	}
}
