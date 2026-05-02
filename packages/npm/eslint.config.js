import { configs, withStyles } from '@mscharley/eslint-config';

export default [
	...configs.recommended,
	...configs.node,
	...withStyles(),
	{
		rules: {
			// process.exit() is legitimate in postinstall scripts to signal failure to npm
			'n/no-process-exit': 'off',
			// src/*.ts compiles to bin/*.js; cursus.shim.js is the spawner that postinstall
			// copies over bin/cursus.js at install time, so it legitimately needs a shebang
			// even though it is not directly referenced by package.json#bin.
			'n/hashbang': ['error', {
				convertPath: { 'src/**/*.ts': ['^src/(.+)\\.ts$', 'bin/$1.js'] },
				additionalExecutables: ['bin/cursus.shim.js'],
			}],
		},
	},
];
