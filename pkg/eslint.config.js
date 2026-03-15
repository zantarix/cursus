import { configs, withStyles } from '@mscharley/eslint-config';

export default [
	...configs.recommended,
	...configs.node,
	...withStyles(),
	{
		rules: {
			// process.exit() is legitimate in postinstall scripts to signal failure to npm
			'n/no-process-exit': 'off',
		},
	},
];
