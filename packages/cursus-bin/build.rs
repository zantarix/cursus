use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	emit_workspace_root();
	generate_windows_synchronization_lib()?;

	Ok(())
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

const SYNCHRONIZATION_DEF: &str = "LIBRARY synchronization.dll

EXPORTS
DeleteSynchronizationBarrier
EnterSynchronizationBarrier
InitializeSynchronizationBarrier
InitOnceBeginInitialize
InitOnceComplete
InitOnceExecuteOnce
InitOnceInitialize
SignalObjectAndWait
Sleep
SleepConditionVariableCS
SleepConditionVariableSRW
WaitOnAddress
WakeAllConditionVariable
WakeByAddressAll
WakeByAddressSingle
WakeConditionVariable
";

/// Generates a `synchronization.lib` import library for Windows targets using `zig dlltool`.
///
/// zig 0.15.2 does not bundle `synchronization.lib`, so `cargo zigbuild` for Windows targets
/// fails to link symbols such as `WaitOnAddress`. This generates the import library at build
/// time instead of committing binary blobs to the repository.
///
/// Is a no-op for non-Windows targets.
///
/// Upstream reference: <https://github.com/ziglang/zig/issues/14919>
fn generate_windows_synchronization_lib() -> Result<(), Box<dyn std::error::Error>> {
	let target = std::env::var("TARGET").map_err(|_| "TARGET env var not set by Cargo")?;

	if !target.contains("windows") {
		return Ok(());
	}

	let arch = if target.starts_with("x86_64") {
		"i386:x86-64"
	} else if target.starts_with("aarch64") {
		"arm64"
	} else {
		return Err(format!("unsupported Windows target architecture: {target}").into());
	};

	let out_dir = std::env::var("OUT_DIR")?;
	let def_path = format!("{out_dir}/synchronization.def");
	let lib_path = format!("{out_dir}/libsynchronization.a");

	std::fs::write(&def_path, SYNCHRONIZATION_DEF)?;

	let output = Command::new("zig")
		.args([
			"dlltool",
			"--input-def",
			&def_path,
			"--output-lib",
			&lib_path,
			"-m",
			arch,
		])
		.output()?;

	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr);
		return Err(format!("zig dlltool failed ({}): {stderr}", output.status).into());
	}

	println!("cargo:rustc-link-search=native={out_dir}");
	Ok(())
}
