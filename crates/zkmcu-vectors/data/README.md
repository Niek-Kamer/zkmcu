# Test vectors

Binary files in this directory are produced by `cargo run -p zkmcu-host-gen --release` and committed for reproducibility. They are in the EIP-197 binary format documented at the root README.

| Directory | Circuit | Public inputs |
|-----------|---------|---------------|
| `square/` | `x^2 = y` — proves knowledge of a square root | 1 |
