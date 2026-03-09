// zig 0.15.2 does not bundle synchronization.lib, so `cargo zigbuild` for
// Windows targets fails to link symbols such as WaitOnAddress. We generate the
// import library at build time via `zig dlltool` instead of committing binary
// blobs to the repository.
//
// Upstream reference: https://github.com/ziglang/zig/issues/14919

use std::process::Command;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
	println!("cargo:rerun-if-changed=build.rs");

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
