import { parse } from 'smol-toml';

// Vite bundles each matched TOML's raw contents at build time, so the values
// travel inside the JS bundle itself. No runtime filesystem access, no
// `import.meta.url` path fragility between dev (source location) and build
// (bundled location), and no Cloudflare-runtime node:fs dependency if this
// ever ships under a server adapter.
//
// Glob is resolved relative to THIS source file, so the path is stable no
// matter where Vite writes the chunk.
const rawToml = import.meta.glob<string>(
	'../../../benchmarks/runs/*/result.toml',
	{ query: '?raw', import: 'default', eager: true },
);

const bySlug: Record<string, string> = {};
for (const [path, contents] of Object.entries(rawToml)) {
	const m = /benchmarks\/runs\/([^/]+)\/result\.toml$/.exec(path);
	const slug = m?.[1];
	if (slug !== undefined) {
		bySlug[slug] = contents;
	}
}

export interface Bench {
	cycles_median?: number;
	cycles_min?: number;
	cycles_max?: number;
	cycles_typical?: number;
	us_median?: number;
	us_typical?: number;
	iterations?: number;
	result?: string;
	[k: string]: unknown;
}

export interface Run {
	meta: {
		date: string;
		target: string;
		toolchain: string;
		profile: string;
		commit?: string;
	};
	hardware: {
		board: string;
		cpu: string;
		clock_hz: number;
		sram_bytes: number;
		flash_bytes: number;
	};
	libraries: Record<string, string>;
	footprint: {
		text_bytes: number;
		data_bytes: number;
		bss_bytes: number;
		heap_bytes: number;
		stack_peak_bytes?: number;
	};
	vector?: { proof_size_bytes?: number; [k: string]: unknown };
	bench: Record<string, Bench>;
}

const cache = new Map<string, Run>();

export function loadRun(slug: string): Run {
	let run = cache.get(slug);
	if (!run) {
		const raw = bySlug[slug];
		if (!raw) {
			throw new Error(`no result.toml bundled for run slug "${slug}"`);
		}
		run = parse(raw) as unknown as Run;
		cache.set(slug, run);
	}
	return run;
}

export function us(slug: string, benchName: string): number {
	const bench = loadRun(slug).bench[benchName];
	if (!bench) {
		throw new Error(`bench ${benchName} not found in ${slug}`);
	}
	const v = bench.us_median ?? bench.us_typical;
	if (v === undefined) {
		throw new Error(`no us_median/us_typical for ${slug}/${benchName}`);
	}
	return v;
}

export function proofBytes(slug: string): number {
	const b = loadRun(slug).vector?.proof_size_bytes;
	if (b === undefined) {
		throw new Error(`no vector.proof_size_bytes in ${slug}`);
	}
	return b;
}
