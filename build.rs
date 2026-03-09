use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
	fetch_github_openapi_spec();
	generate_windows_synchronization_lib()?;

	Ok(())
}

const GITHUB_OPENAPI_URL: &str = "https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.2022-11-28.json";

/// Fetches and caches the GitHub OpenAPI spec for use in integration tests.
///
/// Writes the spec to `.cache/github-openapi.json` relative to the crate root.
/// Refreshes the cache if the file is older than 7 days.
/// Fails silently if the network is unavailable.
fn fetch_github_openapi_spec() {
	let manifest_dir = match std::env::var("CARGO_MANIFEST_DIR") {
		Ok(d) => d,
		Err(_) => return,
	};

	let cache_dir = format!("{manifest_dir}/.cache");
	let spec_path = format!("{cache_dir}/github-openapi.json");

	let needs_fetch = match std::fs::metadata(&spec_path) {
		Ok(meta) => {
			if let Ok(modified) = meta.modified() {
				let age = std::time::SystemTime::now()
					.duration_since(modified)
					.unwrap_or(std::time::Duration::from_secs(u64::MAX));
				age > std::time::Duration::from_secs(7 * 24 * 3600)
			} else {
				true
			}
		}
		Err(_) => true,
	};

	if !needs_fetch {
		return;
	}

	if std::fs::create_dir_all(&cache_dir).is_err() {
		return;
	}

	// Inform Cargo that deleting the cache file should trigger a re-fetch.
	println!("cargo:rerun-if-changed={spec_path}");

	match ureq::get(GITHUB_OPENAPI_URL).call() {
		Ok(mut response) => match response.body_mut().read_to_string() {
			Ok(body) if !body.is_empty() => {
				let _ = std::fs::write(&spec_path, body);
			}
			_ => {}
		},
		Err(_) => {
			// Network unavailable — integration tests that require the spec will skip.
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
