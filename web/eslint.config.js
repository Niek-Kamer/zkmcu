// Flat ESLint config for the zkmcu docs site.
//
// Scope: type-aware TypeScript rules + Astro template rules. Biome handles
// formatting, quote style, semicolons, unused imports, and the baseline
// lint rules it can parse. ESLint's job here is the type-checking-based
// rules Biome can't do, plus .astro files wich Biome doesn't parse yet.
//
// Strictness target: tsconfig is astro/tsconfigs/strictest + typescript-eslint
// strictTypeChecked + stylisticTypeChecked + eslint-plugin-astro recommended
// + jsx-a11y strict. Every rule is at error severity so CI fails on any
// regression. Override locally with `// eslint-disable-next-line ...` only
// with a real reason, not just to make CI pass.

import js from '@eslint/js';
import astro from 'eslint-plugin-astro';
import tseslint from 'typescript-eslint';

export default tseslint.config(
	{
		// Never lint generated / third-party / vendored trees.
		ignores: [
			'dist/**',
			'.astro/**',
			'node_modules/**',
			'public/**',
			'bun.lock',
		],
	},
	// Baseline JS recommended.
	js.configs.recommended,
	// Type-aware TypeScript rules, both correctness-strict and stylistic.
	...tseslint.configs.strictTypeChecked,
	...tseslint.configs.stylisticTypeChecked,
	{
		// Tell typescript-eslint how to find the project for type-aware rules.
		// projectService: true auto-detects tsconfig from each file, wich is
		// the recommended setup for mono-tsconfig projects like this one.
		// astro-eslint-parser silently downgrades this to `project: true` for
		// .astro files (prints an informational console line at startup, not
		// a lint error, and behaviour is identical). Keep projectService
		// global so type-aware rules have parser services available on every
		// file they target, including .astro.
		languageOptions: {
			parserOptions: {
				projectService: true,
				tsconfigRootDir: import.meta.dirname,
				extraFileExtensions: ['.astro'],
			},
		},
	},
	// Astro templates + jsx-a11y strict config baked into the Astro plugin.
	...astro.configs.recommended,
	...astro.configs['jsx-a11y-strict'],
	{
		// Biome handles formatting, unused imports, and quote style, so turn
		// off the ESLint equivalents. No point in having the two tools argue.
		rules: {
			'no-extra-semi': 'off',
			semi: 'off',
			quotes: 'off',
			indent: 'off',
			'@typescript-eslint/no-unused-vars': [
				'error',
				{
					argsIgnorePattern: '^_',
					varsIgnorePattern: '^_',
					caughtErrorsIgnorePattern: '^_',
				},
			],
			// Fails hard on console.log. console.warn and console.error still
			// allowed for genuine runtime diagnostics.
			'no-console': ['error', { allow: ['warn', 'error'] }],
			// Numbers in template literals are safe and common in this repo
			// ("${ms} ms", "${pct}%"). The default rule disallows them, so
			// allow number + boolean + nullish explicitly. Saves a bunch of
			// string-coercion churn polluting the codebase.
			'@typescript-eslint/restrict-template-expressions': [
				'error',
				{
					allowNumber: true,
					allowBoolean: true,
					allowNullish: true,
				},
			],
		},
	},
	{
		// .astro files go through astro-eslint-parser, wich doesn't yet
		// carry full TypeScript program info into the template. That makes
		// the no-unsafe-* family throw on plain `Astro.props` usage. False
		// positives, not real bugs. Disable that family at the .astro level
		// only, frontmatter .ts code still gets the full check when used
		// from a .ts file. Unused-vars is off because Astro treats
		// frontmatter as module state referenced by the template.
		files: ['**/*.astro'],
		rules: {
			'@typescript-eslint/no-unused-vars': 'off',
			'@typescript-eslint/no-unsafe-return': 'off',
			'@typescript-eslint/no-unsafe-call': 'off',
			'@typescript-eslint/no-unsafe-member-access': 'off',
			'@typescript-eslint/no-unsafe-assignment': 'off',
			'@typescript-eslint/no-unsafe-argument': 'off',
		},
	},
	{
		// Config files themselves aren't type-checked by tsconfig, so opt
		// them out of type-aware rules to avoid parser errors.
		files: ['*.config.{js,mjs,ts}', 'eslint.config.js'],
		...tseslint.configs.disableTypeChecked,
	},
);
