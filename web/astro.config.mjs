// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://zkmcu.dev',
	integrations: [
		starlight({
			title: 'zkmcu',
			description:
				'no_std Rust Groth16/BN254 verifier for ARM Cortex-M and RISC-V microcontrollers.',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/Niek-Kamer/zkmcu',
				},
			],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{ label: 'Home', link: '/' },
				{ label: 'Getting started', link: '/getting-started/' },
				{ label: 'Architecture', link: '/architecture/' },
				{ label: 'Wire format', link: '/wire-format/' },
				{ label: 'Benchmarks', link: '/benchmarks/' },
				{ label: 'Security', link: '/security/' },
			],
			editLink: {
				baseUrl: 'https://github.com/Niek-Kamer/zkmcu/edit/main/web/',
			},
			lastUpdated: true,
		}),
	],
});
