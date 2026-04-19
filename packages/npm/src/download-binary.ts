import { chmod, readFile, unlink, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));

interface PackageJson {
	version: string;
	bin: string;
	[key: string]: unknown;
}

const pkg = JSON.parse(await readFile(join(__dirname, '..', 'package.json'), 'utf-8')) as PackageJson;
const { version } = pkg;

const PLATFORMS: Partial<Record<string, Partial<Record<string, string>>>> = {
	linux: {
		x64: 'cursus-linux-x86_64',
		arm64: 'cursus-linux-aarch64',
		riscv64: 'cursus-linux-riscv64gc',
	},
	darwin: {
		x64: 'cursus-osx-x86_64',
		arm64: 'cursus-osx-aarch64',
	},
	win32: {
		x64: 'cursus-windows-x86_64.exe',
		arm64: 'cursus-windows-aarch64.exe',
	},
};

const platform = process.platform;
const arch = process.arch;
const artifact = PLATFORMS[platform]?.[arch];
const tag = `cursus@${version}`;

if (artifact == null) {
	const supported = Object.entries(PLATFORMS)
		.flatMap(([p, arches]) => Object.keys(arches ?? {}).map((a) => `${p}/${a}`))
		.join(', ');
	process.stderr.write(
		`Error: Unsupported platform ${platform}/${arch}.\n`
		+ `Supported platforms: ${supported}\n`
		+ `Please download manually from: https://github.com/zantarix/cursus/releases/tag/${encodeURIComponent(tag)}\n`,
	);
	process.exit(1);
}

const isWindows = platform === 'win32';
const binaryName = isWindows ? 'cursus.exe' : 'cursus';
const binaryPath = join(__dirname, binaryName);
const downloadUrl = `https://github.com/zantarix/cursus/releases/download/${encodeURIComponent(tag)}/${artifact}`;

const TIMEOUT_MS = 60_000;
const UNIX_EXECUTABLE_MODE = 0o755;

async function download(fileUrl: string, dest: string): Promise<void> {
	const controller = new AbortController();
	const timer = setTimeout(() => {
		controller.abort();
	}, TIMEOUT_MS);

	try {
		const response = await fetch(fileUrl, { signal: controller.signal });

		if (!response.url.startsWith('https://')) {
			throw new Error(`Refusing non-HTTPS redirect to ${response.url}`);
		}

		if (!response.ok) {
			throw new Error(`HTTP ${response.status.toString()} fetching ${response.url}`);
		}

		const bytes = await response.arrayBuffer();
		try {
			await writeFile(dest, Buffer.from(bytes));
		} catch (err) {
			try {
				await unlink(dest);
			} catch {
				// Ignore cleanup errors
			}
			throw err;
		}
	} finally {
		clearTimeout(timer);
	}
}

process.stdout.write(`Downloading cursus v${version} for ${platform}/${arch}...\n`);

try {
	await download(downloadUrl, binaryPath);

	if (isWindows) {
		// Update the bin field so that subsequent npx invocations resolve the .exe correctly.
		const pkgPath = join(__dirname, '..', 'package.json');
		const pkgData = JSON.parse(await readFile(pkgPath, 'utf-8')) as PackageJson;
		pkgData.bin = 'bin/cursus.exe';
		await writeFile(pkgPath, `${JSON.stringify(pkgData, null, '\t')}\n`);
	} else {
		await chmod(binaryPath, UNIX_EXECUTABLE_MODE);
	}

	process.stdout.write(`Successfully installed cursus to ${binaryPath}\n`);
} catch (err) {
	const message = err instanceof Error ? err.message : String(err);
	process.stderr.write(
		`Error: Failed to download cursus binary: ${message}\n`
		+ `Please try again or download manually from: https://github.com/zantarix/cursus/releases/tag/${encodeURIComponent(tag)}\n`,
	);
	process.exit(1);
}
