# 2026-04-27 — M33 Groth16 timing oracle (BN254, square circuit)

So I wanted to actually check whether the verifier leaks timing information in a way that a real attacker could use. Like, it's one thing to say "early exit exists" but another to measure it over USB with 1000 samples and see if the distributions are even separable. Turns out they really are.

## What this is

A network timing oracle running on the Pico 2 W. Host sends a 2-byte knock (`\x55\xAA`) followed by 256 bytes of proof, firmware verifies on-device and responds with `<verdict> <cycles> <us>`. Protocol is dead simple and adding the knock byte solves the Linux tty echo race wich was eating the first few proof bytes on the previous attempts.

Tested 6 proof categories, 1000 samples each.

## Results

| category | verdict | mean (µs) | std (µs) | cv | interpretation |
|---|---|---:|---:|---:|---|
| valid | T | 632 500 | 32.84 | 0.005% | full verify |
| bit_flip_byte_255 | E | 90 485 | 7.96 | 0.009% | C on-curve/subgroup check after A+B pass |
| all_zeros | F | 382 403 | 30.61 | 0.008% | parses fine, pairing fails |
| bit_flip_byte_127 | E | 205 | 4.42 | 2.150% | B Fp2 element > modulus |
| bit_flip_byte_0 | E | 46 | 2.85 | 6.098% | A G1 curve check |
| random_bytes | E | 15 | 1.67 | 10.593% | first Fq element > modulus, bails immediately |

These are four orders of magnitude apart. This isn't subtle noise you'd argue about in a paper, it's just obviously distinguishable.

## What the timing ladder tells you

The categories split cleanly by where in the verify path the error fires:

**15 µs — random_bytes.** First 32 bytes of a random proof almost certainly produce an Fq element above the BN254 modulus. Deserializer catches it immediately, no curve math at all.

**46 µs — bit_flip_byte_0.** Byte 0 is A.x[0]. A single bit flip can still produce a valid Fq element (< modulus), so field deserialization passes, but the resulting point is not on y² = x³ + 3. Cheap G1 on-curve check fires.

**205 µs — bit_flip_byte_127.** Byte 127 is the last byte of B.x.c1 (Fp2 component). Flip bit 7 and you get an element above the Fp modulus — fails the field deserialize before any curve math. A is fully parsed and checked before we get here, hence the extra ~160 µs vs byte_0.

**90 485 µs — bit_flip_byte_255.** This one is the interesting case. Byte 255 is the last byte of C.y. Flipping bit 0 keeps C.y below the modulus, so it passes Fq deserialization. A and B also parse OK. Then at some point after parsing all three points the verifier does a more expensive check on C — either a G1 subgroup check (scalar mult by the group order) or the on-curve check fires deeper in the computation. Either way it costs ~90 ms, wich is roughly the cost of a G1 scalar multiplication on this hardware.

**382 403 µs — all_zeros.** The infinity point is technically on the BN254 curve, so all three points parse without error. The verifier runs the full Groth16 pairing equation and only then finds it doesn't hold. Returns F (verify failed) rather than E (parse error). Takes 60% of full-verify time, which makes sense — the infinity point likely shortcuts some Miller loop iterations.

**632 500 µs — valid.** Full verify. cv=0.005%, ±33 µs over 1000 runs. Extremely deterministic.

## Variance pattern

The tight deterministic categories (valid, bit_flip_byte_255, all_zeros) have cv < 0.01% — same code path every single time, no branch variance in the expensive math. The fast E-failures (byte_0, random_bytes) have cv=6–10% just because USB interrupt jitter dominates when the absolute time is 15–46 µs.

## Timing oracle implication

An attacker with USB access can determine:
- Whether A, B, C each deserialize as valid field elements (three distinct time bands under 300 µs).
- Whether A, B parse as valid curve points while C does not (90 ms band).
- Whether the whole proof parses but the pairing fails (382 ms vs 632 ms).

The verifier is not constant-time and doesn't try to be. For an embedded verify-and-display use case (like a hardware wallet showing "proof OK") this would be a real side channel if you controlled what proof bytes got submitted. For our benchmark purposes it just confirms the infrastructure works — clean byte alignment, stable cycle counts, no protocol bugs.

## Infrastructure notes

The knock byte protocol (`\x55\xAA` scanned in firmware, prepended by host to every write) was the fix for a persistent loopback bug. When a process opens `/dev/ttyACM0` on Linux the tty line discipline echoes the firmware's own boot messages back to it. The firmware's drain ran at boot before any process opened the port, so it found nothing. The knock-wait loop discards all echo bytes before the proof read, no matter how many loopback bytes arrive. No timing race possible.

Sync flow: host drains kernel RX buffer (0.5 s timeout), then sends knock + valid proof, waits for T/F/E response. Works on both fresh-boot and late-connect.

## Reproduction

```bash
# dev machine
cargo build --release  # in crates/bench-rp2350-m33-timing-oracle/
scp target/thumbv8m.main-none-eabihf/release/bench-rp2350-m33-timing-oracle pid-admin@10.42.0.30:/tmp/bench-timing-oracle.elf
scp scripts/timing_oracle.py pid-admin@10.42.0.30:~/timing_oracle.py

# Pi 5 — put Pico in BOOTSEL first
picotool load -v -x -t elf /tmp/bench-timing-oracle.elf
python3 ~/timing_oracle.py --port /dev/ttyACM0 --samples 1000
```
