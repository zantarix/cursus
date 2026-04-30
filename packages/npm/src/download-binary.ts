import { type Bundle, verify } from 'sigstore';
import { chmod, readFile, unlink, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { createHash } from 'node:crypto';
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
const MAX_REDIRECTS = 5;
const MAX_BINARY_BYTES = 50 * 1024 * 1024;
const MAX_ATTESTATION_BYTES = 10 * 1024 * 1024;

async function readBounded(response: Response, maxBytes: number): Promise<Buffer> {
	const reader = response.body?.getReader();
	if (!reader) {
		throw new Error('Response body is not readable');
	}
	let total = 0;
	const chunks: Uint8Array[] = [];
	try {
		while (true) {
			// eslint-disable-next-line no-await-in-loop
			const { done, value } = await reader.read();
			if (done) break;
			total += value.byteLength;
			if (total > maxBytes) {
				throw new Error(`Response body exceeds ${maxBytes} byte limit`);
			}
			chunks.push(value);
		}
	} finally {
		reader.releaseLock();
	}
	return Buffer.concat(chunks);
}

const ALLOWED_DOWNLOAD_HOSTS = new Set([
	'github.com',
	'objects.githubusercontent.com',
	'release-assets.githubusercontent.com',
]);

async function downloadBuffer(fileUrl: string): Promise<Buffer> {
	const controller = new AbortController();
	const timer = setTimeout(() => {
		controller.abort();
	}, TIMEOUT_MS);

	try {
		let currentUrl = fileUrl;
		for (let i = 0; i <= MAX_REDIRECTS; i++) {
			// eslint-disable-next-line no-await-in-loop
			const response = await fetch(currentUrl, {
				signal: controller.signal,
				redirect: 'manual',
			});

			if (response.status >= 300 && response.status < 400) {
				const location = response.headers.get('location');
				if (!location) {
					throw new Error('Redirect response missing Location header');
				}
				const next = new URL(location, currentUrl);
				if (next.protocol !== 'https:') {
					throw new Error(`Refusing non-HTTPS redirect to ${next.href}`);
				}
				if (!ALLOWED_DOWNLOAD_HOSTS.has(next.hostname)) {
					throw new Error(`Refusing redirect to disallowed host ${next.hostname}`);
				}
				currentUrl = next.href;
				continue;
			}

			if (!response.ok) {
				throw new Error(`HTTP ${response.status.toString()} fetching ${currentUrl}`);
			}

			return readBounded(response, MAX_BINARY_BYTES);
		}
		throw new Error(`Too many redirects fetching ${fileUrl}`);
	} finally {
		clearTimeout(timer);
	}
}

interface DsseEnvelope {
	payload?: string;
}

interface AttestationBundle {
	dsseEnvelope?: DsseEnvelope;
	[key: string]: unknown;
}

interface InTotoStatement {
	subject?: Array<{ digest?: Record<string, string> }>;
}

// All artifacts are built and attested in release-artifacts.yml, which triggers on
// release: published and therefore always carries refs/tags/cursus@<version>.
function expectedWorkflow(): string {
	return 'release-artifacts.yml';
}

async function verifyAttestation(buffer: Buffer): Promise<void> {
	const digest = createHash('sha256').update(buffer).digest('hex');

	const controller = new AbortController();
	const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

	let bundles: AttestationBundle[];
	try {
		const url = `https://api.github.com/repos/zantarix/cursus/attestations/sha256:${digest}`;
		const response = await fetch(url, {
			headers: { Accept: 'application/vnd.github+json' },
			signal: controller.signal,
		});
		if (!response.ok) {
			const isRateLimited = response.status === 403
				&& response.headers.get('x-ratelimit-remaining') === '0';
			if (isRateLimited) {
				const retryAfter = response.headers.get('retry-after');
				let when = '';
				if (retryAfter != null) {
					const secs = parseInt(retryAfter, 10);
					const formatted = !isNaN(secs) && secs >= 60
						? `${Math.ceil(secs / 60)} minute(s)`
						: `${retryAfter} second(s)`;
					when = ` Try again in ${formatted}.`;
				}
				throw new Error(
					`GitHub API rate limit exceeded (unauthenticated: 60 req/hr).${when} `
					+ 'Alternatively, run from a network with a different egress IP.',
				);
			}
			throw new Error(`HTTP ${response.status.toString()} fetching attestation`);
		}
		const data = JSON.parse((await readBounded(response, MAX_ATTESTATION_BYTES)).toString('utf-8')) as { attestations?: Array<{ bundle: AttestationBundle }> };
		const attestations = data.attestations ?? [];
		if (attestations.length === 0) {
			throw new Error(`No attestation found for digest sha256:${digest}`);
		}
		bundles = attestations.map((a) => a.bundle);
	} finally {
		clearTimeout(timer);
	}

	const workflow = expectedWorkflow();
	// `version` equals the cursus release version because the npm package version is
	// locked to the release version. The attestation is therefore always issued
	// against the tag that corresponds to this exact install.
	const certIdentityURI = `https://github.com/zantarix/cursus/.github/workflows/${workflow}@refs/tags/cursus@${version}`;

	const results = await Promise.allSettled(bundles.map(async (bundle) => {
		await verify(bundle as Bundle, {
			certificateIssuer: 'https://token.actions.githubusercontent.com',
			certificateIdentityURI: certIdentityURI,
		});

		// Confirm the attested subject digest matches the downloaded binary.
		const payloadB64 = bundle.dsseEnvelope?.payload;
		if (payloadB64 == null) {
			throw new Error('Attestation bundle missing DSSE envelope payload');
		}
		const statement = JSON.parse(
			Buffer.from(payloadB64, 'base64').toString('utf-8'),
		) as InTotoStatement;
		const digestMatch = (statement.subject ?? []).some((s) => s.digest?.sha256?.toLowerCase() === digest);
		if (!digestMatch) {
			throw new Error('Attestation subject digest does not match the downloaded binary');
		}
	}));

	if (results.some((r) => r.status === 'fulfilled')) {
		return;
	}

	// PromiseRejectedResult.reason is typed as `any` in TypeScript's built-in types.
	const firstRejected = results.find((r): r is PromiseRejectedResult => r.status === 'rejected');
	const reason: unknown = firstRejected?.reason;
	const msg = reason instanceof Error ? reason.message : 'Unknown error';
	throw new Error(`Attestation verification failed: ${msg}`);
}

process.stdout.write(`Downloading cursus v${version} for ${platform}/${arch}...\n`);

let bytes: Buffer;
try {
	bytes = await downloadBuffer(downloadUrl);
} catch (err) {
	const message = err instanceof Error ? err.message : String(err);
	process.stderr.write(
		`Error: Failed to download cursus binary: ${message}\n`
		+ `Please try again or download manually from: https://github.com/zantarix/cursus/releases/tag/${encodeURIComponent(tag)}\n`,
	);
	process.exit(1);
}

try {
	await verifyAttestation(bytes);
} catch (err) {
	// No on-disk cleanup needed: `writeFile(binaryPath, bytes)` has not been called
	// yet, so nothing has been written to the install location.
	const message = err instanceof Error ? err.message : String(err);
	process.stderr.write(
		`Error: ${message}\n`
		+ `The binary may not be genuine. Do not use it and report this issue at https://github.com/zantarix/cursus/issues\n`
		+ `Manual download: https://github.com/zantarix/cursus/releases/tag/${encodeURIComponent(tag)}\n`,
	);
	process.exit(1);
}

try {
	await writeFile(binaryPath, bytes);

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
		`Error: Failed to install cursus binary: ${message}\n`
		+ `Please try again or download manually from: https://github.com/zantarix/cursus/releases/tag/${encodeURIComponent(tag)}\n`,
	);
	try {
		await unlink(binaryPath);
	} catch {
		// ignore
	}
	process.exit(1);
}
