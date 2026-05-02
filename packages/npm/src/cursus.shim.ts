#!/usr/bin/env node
import { dirname, join } from 'node:path';
import { constants } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const binaryPath = join(__dirname, process.platform === 'win32' ? 'cursus.exe' : 'cursus-bin');

const child = spawn(binaryPath, process.argv.slice(2), { stdio: 'inherit' });

process.on('SIGTERM', () => child.kill('SIGTERM'));
process.on('SIGINT', () => child.kill('SIGINT'));
process.on('SIGHUP', () => child.kill('SIGHUP'));

child.on('error', (err) => {
	if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
		process.stderr.write(
			'cursus: native binary is not installed.\n'
			+ 'The postinstall step did not complete successfully on this machine.\n'
			+ '\n'
			+ 'To retry, run:\n'
			+ '  npm rebuild @zantarix/cursus\n'
			+ '\n'
			+ 'Or download the binary manually from:\n'
			+ '  https://github.com/zantarix/cursus/releases\n',
		);
	} else {
		process.stderr.write(`cursus: failed to spawn binary: ${err.message}\n`);
	}
	process.exit(1);
});

child.on('exit', (code, signal) => {
	if (code != null) {
		process.exit(code);
	} else if (signal != null) {
		process.exit(128 + constants.signals[signal]);
	} else {
		process.exit(1);
	}
});
