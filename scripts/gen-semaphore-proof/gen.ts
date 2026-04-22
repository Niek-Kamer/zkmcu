// One-shot deterministic Semaphore proof generator for the zkmcu benchmark.
//
// Reads no arguments — seeds, message, scope, depth are hardcoded below so
// rerunning this script always produces byte-identical output. The proof
// it generates is valid under the Semaphore depth-10 VK already extracted
// at crates/zkmcu-vectors/data/semaphore-depth-10/vk.bin.
//
// Run with:   bun run gen
// Outputs:    proof.json (in this directory)
// Consume:    cargo run -p zkmcu-host-gen --release -- semaphore \
//               --depth 10 --proof scripts/gen-semaphore-proof/proof.json

import { Group } from "@semaphore-protocol/group"
import { Identity } from "@semaphore-protocol/identity"
import { generateProof, verifyProof } from "@semaphore-protocol/proof"
import { encodeBytes32String } from "ethers/abi"
import { keccak256 } from "ethers/crypto"
import { toBeHex, toBigInt as ethersToBigInt } from "ethers/utils"
import { writeFileSync } from "node:fs"

// ---- Deterministic inputs ----------------------------------------------

const SEED = "zkmcu-bench-seed"
const MESSAGE = "zkmcu-bench-message"
const SCOPE = "zkmcu-bench-scope"
const DEPTH = 10

// ---- Replicate Semaphore's internal transforms -------------------------

/** Mirror of @semaphore-protocol/proof/src/to-bigint.ts. */
function toBigInt(value: string | bigint | number): bigint {
  try {
    return ethersToBigInt(value as never)
  } catch {
    return ethersToBigInt(encodeBytes32String(value as string))
  }
}

/** Mirror of @semaphore-protocol/proof/src/hash.ts. */
function hashToField(value: string | bigint | number): string {
  const bn = toBigInt(value)
  return ((BigInt(keccak256(toBeHex(bn, 32))) >> 8n)).toString()
}

// ---- Main --------------------------------------------------------------

const identity = new Identity(SEED)
const group = new Group([1n, 2n, identity.commitment])

console.error(`identity commitment : ${identity.commitment}`)
console.error(`group merkle root   : ${group.root}`)

const proof = await generateProof(identity, group, MESSAGE, SCOPE, DEPTH)

// Self-check: the Semaphore verifier must accept its own proof. If this
// fails, something is wrong with our inputs or the downloaded snark-
// artifacts — no point writing proof.json.
const ok = await verifyProof(proof)
if (!ok) {
  console.error("FATAL: Semaphore's own verifier rejected the proof")
  process.exit(1)
}
console.error("semaphore local verify : ok")

// Reconstruct the public-signals array in the order snarkjs uses:
//   [merkleTreeRoot, nullifier, hash(message), hash(scope)]
// (see @semaphore-protocol/proof/src/verify-proof.ts).
const publicSignals = [
  proof.merkleTreeRoot,
  proof.nullifier,
  hashToField(MESSAGE),
  hashToField(SCOPE),
]

// Packed Groth16 proof is 8 decimal strings already in EVM / EIP-197 order:
//   [A.x, A.y, B.x.c1, B.x.c0, B.y.c1, B.y.c0, C.x, C.y]
// This matches exactly what our Rust importer expects — no rearrangement.
const output = {
  meta: {
    semaphore_version: "4.14.2",
    snark_artifacts_version: "4.13.0",
    seed: SEED,
    message: MESSAGE,
    scope: SCOPE,
    depth: DEPTH,
    identity_commitment: identity.commitment.toString(),
    merkle_root: proof.merkleTreeRoot,
    nullifier: proof.nullifier,
  },
  proof: proof.points,
  public_signals: publicSignals,
}

writeFileSync("proof.json", JSON.stringify(output, null, 2) + "\n")
console.error(`wrote proof.json (${JSON.stringify(output).length} bytes)`)
