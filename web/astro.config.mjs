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
				'no_std Rust family of SNARK and STARK verifiers for ARM Cortex-M and RISC-V microcontrollers. BN254, BLS12-381, and winterfell STARK — all under 128 KB SRAM.',
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
				{ label: 'Wire formats', link: '/wire-format/' },
				{
					label: 'On silicon',
					items: [
						{ label: 'STARK (75 ms, 100 KB)', link: '/stark/' },
						{ label: 'Semaphore (real-world)', link: '/semaphore/' },
						{ label: 'Benchmarks', link: '/benchmarks/' },
						{ label: 'Deterministic timing', link: '/determinism/' },
					],
				},
				{ label: 'Security', link: '/security/' },
			],
			editLink: {
				baseUrl: 'https://github.com/Niek-Kamer/zkmcu/edit/main/web/',
			},
			lastUpdated: true,
		}),
	],
});
