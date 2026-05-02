#!/usr/bin/env node

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
process.exit(1);
