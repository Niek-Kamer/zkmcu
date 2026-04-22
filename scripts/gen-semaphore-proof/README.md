# gen-semaphore-proof

One-shot Bun / TypeScript script that drives
[@semaphore-protocol/proof](https://www.npmjs.com/package/@semaphore-protocol/proof)
to emit a deterministic Groth16/BN254 Semaphore proof. Pinned to
Semaphore 4.14.2 to match the VK we vendored in `vendor/semaphore`.

This is the only piece of JavaScript in the project — it exists
because the Semaphore proving key + wasm live in npm/CDN artifacts
and there's no Rust-native proof generator we can use to produce a
proof valid under the Semaphore VK.

## What it outputs

`proof.json` in this directory, shape:

```json
{
  "meta": {...},
  "proof": ["A.x", "A.y", "B.x.c1", "B.x.c0", "B.y.c1", "B.y.c0", "C.x", "C.y"],
  "public_signals": ["merkleTreeRoot", "nullifier", "H(message)", "H(scope)"]
}
```

The 8-element `proof` array is already in EVM / EIP-197 order (Fp2 as
`(c1, c0)`), wich matches exactly what `zkmcu-host-gen`'s Rust-side
importer wants — no rearrangement.

## Run

```bash
bun install             # ~50 MB of node_modules, one-time
bun run gen             # fetches snark-artifacts on first run, writes proof.json
```

First run downloads the Semaphore snark-artifacts (wasm + zkey) for the
configured depth from the Semaphore CDN; subsequent runs reuse the
cached copy. Expect \~30 s on first run, \~3 s after.

## Consume

```bash
cd ../..                 # back to repo root
cargo run -p zkmcu-host-gen --release -- semaphore \
    --depth 10 \
    --proof scripts/gen-semaphore-proof/proof.json
```

That writes `crates/zkmcu-vectors/data/semaphore-depth-10/proof.bin`
and `public.bin` alongside the already-extracted `vk.bin`.

## What's gitignored

`node_modules/`, `bun.lockb`, and `proof.json` itself. The proof bytes
that matter for the benchmark live at `crates/zkmcu-vectors/data/
semaphore-depth-10/{proof,public}.bin` after the Rust import runs.
This directory is scratch.
