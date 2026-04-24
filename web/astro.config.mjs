// @ts-check

import starlight from '@astrojs/starlight';
import { defineConfig } from 'astro/config';

// https://astro.build/config
export default defineConfig({
	site: 'https://zkmcu.dev',
	integrations: [
		starlight({
			title: 'zkmcu',
			description:
				'no_std Rust family of SNARK and STARK verifiers for ARM Cortex-M and RISC-V microcontrollers. BN254, BLS12-381, and winterfell STARK, all under 128 KB SRAM.',
			// .ico is the primary icon, widest browser support. Extra sizes +
			// apple-touch-icon + webmanifest are wired via `head` below so we
			// cover mobile home-screen, PWA install, and retina displays too.
			favicon: '/favicon.ico',
			head: [
				{
					tag: 'link',
					attrs: {
						rel: 'icon',
						type: 'image/png',
						sizes: '16x16',
						href: '/favicon-16x16.png',
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'icon',
						type: 'image/png',
						sizes: '32x32',
						href: '/favicon-32x32.png',
					},
				},
				{
					tag: 'link',
					attrs: {
						rel: 'apple-touch-icon',
						sizes: '180x180',
						href: '/apple-touch-icon.png',
					},
				},
				{ tag: 'link', attrs: { rel: 'manifest', href: '/site.webmanifest' } },
				{ tag: 'meta', attrs: { name: 'theme-color', content: '#b07348' } },
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/Niek-Kamer/zkmcu',
				},
			],
			customCss: ['./src/styles/custom.css'],
			components: {
				// Wraps Starlight's default Footer and appends the site-wide
				// brand + links block. See src/components/Footer.astro.
				Footer: './src/components/Footer.astro',
			},
			expressiveCode: {
				// Catppuccin pair gives us saturated, high-contrast syntax in
				// both themes instead of Starlight's default muted pastel.
				// Mocha is the dark theme, Latte is the light theme; expressive
				// code swaps automatically via the `data-theme` attribute.
				themes: ['catppuccin-mocha', 'catppuccin-latte'],
				styleOverrides: {
					// Match code-block chrome to the site's raised-surface
					// (gray-7) so it reads as a card variant, not a foreign
					// terminal dropped onto the page. Border picks up the
					// hairline so it matches card/aside divider color.
					borderColor: 'var(--sl-color-hairline)',
					borderRadius: '8px',
					frames: {
						editorBackground: 'var(--sl-color-gray-7)',
						terminalBackground: 'var(--sl-color-gray-7)',
						editorActiveTabBackground: 'var(--sl-color-gray-6)',
						terminalTitlebarBackground: 'var(--sl-color-gray-6)',
					},
				},
			},
			sidebar: [
				{ label: 'Home', link: '/' },
				{ label: 'Getting started', link: '/getting-started/' },
				{ label: 'Architecture', link: '/architecture/' },
				{ label: 'Wire formats', link: '/wire-format/' },
				{
					label: 'On silicon',
					items: [
						{ label: 'STARK (75 ms, 100 KB)', link: '/stark/' },
						{ label: 'BabyBear × Quartic (cross-ISA 1.04×)', link: '/babybear/' },
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
			// We ship our own themed 404 at src/pages/404.astro — suppress
			// Starlight's default one to avoid the route-collision warning.
			disable404Route: true,
		}),
	],
});
