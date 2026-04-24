// Central registry of benchmark rows referenced by doc-site pages. Each
// entry names the (run slug, bench name) pair that `us()` needs to fetch
// a single number from `benchmarks/runs/*/result.toml` at build time.
//
// When a canonical run gets replaced (e.g. a rebaseline), update the slug
// here and every page picks up the new number automatically. MDX pages
// should call `usAt('<key>')` rather than repeating slug + bench name at
// each call site.

import { us } from './benchmarks';

export const BENCHMARKS = {
	// STARK headline (TlsfHeap, 95-bit Quadratic), the production pick
	stark_m33_tlsf:  { slug: '2026-04-24-m33-stark-fib-1024-q-tlsf',  bench: 'stark_verify_fib_1024_q_tlsf' },
	stark_rv32_tlsf: { slug: '2026-04-24-rv32-stark-fib-1024-q-tlsf', bench: 'stark_verify_fib_1024_q_tlsf' },

	// BN254 Groth16 headline (1 public input, square circuit)
	bn254_m33:  { slug: '2026-04-22-m33-heap-96k-confirmed', bench: 'groth16_verify_square' },
	bn254_rv32: { slug: '2026-04-21-rv32-stack-painted',     bench: 'groth16_verify' },

	// BN254 Groth16 on a real Semaphore v4 depth-10 proof (4 big-scalar public inputs)
	bn254_m33_semaphore:  { slug: '2026-04-22-m33-semaphore-depth10',  bench: 'groth16_verify_semaphore' },
	bn254_rv32_semaphore: { slug: '2026-04-22-rv32-semaphore-depth10', bench: 'groth16_verify_semaphore' },

	// BLS12-381 Groth16 headline (1 public input, square circuit)
	bls12_m33:  { slug: '2026-04-22-m33-bls12-baseline',  bench: 'groth16_verify_square' },
	bls12_rv32: { slug: '2026-04-22-rv32-bls12-baseline', bench: 'groth16_verify_square' },

	// BN254 per-op from the stack-painted firmware runs
	bn254_m33_g1_mul:   { slug: '2026-04-21-m33-stack-painted',  bench: 'g1_mul' },
	bn254_m33_g2_mul:   { slug: '2026-04-21-m33-stack-painted',  bench: 'g2_mul' },
	bn254_m33_pairing:  { slug: '2026-04-21-m33-stack-painted',  bench: 'pairing' },
	bn254_rv32_g1_mul:  { slug: '2026-04-21-rv32-stack-painted', bench: 'g1_mul' },
	bn254_rv32_g2_mul:  { slug: '2026-04-21-rv32-stack-painted', bench: 'g2_mul' },
	bn254_rv32_pairing: { slug: '2026-04-21-rv32-stack-painted', bench: 'pairing' },

	// BLS12-381 per-op from the bls12 baseline runs
	bls12_m33_g1_mul:   { slug: '2026-04-22-m33-bls12-baseline',  bench: 'g1_mul' },
	bls12_m33_g2_mul:   { slug: '2026-04-22-m33-bls12-baseline',  bench: 'g2_mul' },
	bls12_m33_pairing:  { slug: '2026-04-22-m33-bls12-baseline',  bench: 'pairing' },
	bls12_rv32_g1_mul:  { slug: '2026-04-22-rv32-bls12-baseline', bench: 'g1_mul' },
	bls12_rv32_g2_mul:  { slug: '2026-04-22-rv32-bls12-baseline', bench: 'g2_mul' },
	bls12_rv32_pairing: { slug: '2026-04-22-rv32-bls12-baseline', bench: 'pairing' },
} as const;

export type BenchKey = keyof typeof BENCHMARKS;

export function usAt(key: BenchKey): number {
	const { slug, bench } = BENCHMARKS[key];
	return us(slug, bench);
}
