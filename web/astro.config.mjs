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
				'no_std Rust family of Groth16 SNARK verifiers for ARM Cortex-M and RISC-V microcontrollers. Supports BN254 (EIP-197) and BLS12-381 (EIP-2537).',
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
				{ label: 'Benchmarks', link: '/benchmarks/' },
				{ label: 'Semaphore (real-world)', link: '/semaphore/' },
				{ label: 'Security', link: '/security/' },
			],
			editLink: {
				baseUrl: 'https://github.com/Niek-Kamer/zkmcu/edit/main/web/',
			},
			lastUpdated: true,
		}),
	],
});
