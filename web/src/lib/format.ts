// Numeric formatters shared across doc-site components and MDX pages.
// Lives apart from `benchmarks.ts` so components that render numbers
// without touching TOML (e.g. footprint tables, summary banners) don't
// need to pull the whole bench-loader graph for a one-line formatter.

export function fmtMs(us: number): string {
	return (us / 1000).toLocaleString('en-US', { maximumFractionDigits: 0 });
}

export function fmtProof(bytes: number): string {
	return bytes >= 1000 ? `${(bytes / 1000).toFixed(1)} KB` : `${bytes} B`;
}
