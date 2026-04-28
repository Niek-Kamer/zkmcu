#!/usr/bin/env python3
"""
Network timing oracle for zkmcu-verifier on Cortex-M33.

Protocol: send exactly 256 bytes (proof), receive one line:
    <verdict> <cycles> <us>\r\n
where verdict is T (ok=true), F (ok=false), E (parse error).

Run after flashing bench-rp2350-m33-timing-oracle and waiting for READY.

Usage:
    python3 scripts/timing_oracle.py [--port /dev/ttyACM0] [--samples 1000]
"""

import argparse
import random
import statistics
import sys
import time

import serial

PROOF_SIZE = 256
PROBE_TIMEOUT = 8  # seconds to wait for a response to the first probe


def sync_device(port: serial.Serial) -> None:
    """Drain stale kernel RX bytes, then send a knock+probe and verify the response."""
    print("  syncing ...", end="", flush=True)

    # Drain any stale bytes (boot messages, tty echo) left in the kernel buffer.
    port.timeout = 0.5
    while port.read(1024):
        pass
    port.timeout = 3

    print()

    # Probe: knock + reference proof → expect T/F/E response.
    port.write(b"\x55\xaa" + VALID_PROOF)
    port.flush()
    line = port.readline().strip().decode(errors="replace")
    if not line:
        raise TimeoutError(f"no response to sync probe (timeout={port.timeout}s)")
    verdict = line.split()[0] if line.split() else ""
    if verdict not in ("T", "F", "E"):
        raise ValueError(f"unexpected sync response: {line!r}")
    print(f"  sync probe: {line}")


def query(port: serial.Serial, proof: bytes) -> tuple[str, int, int]:
    """Send a 256-byte proof, return (verdict, cycles, us)."""
    assert len(proof) == PROOF_SIZE
    port.write(b"\x55\xaa" + proof)
    port.flush()
    # readline() blocks until \n or timeout avoids the in_waiting race where
    # a fast parse-error response arrives before Python checks in_waiting.
    line = port.readline().strip().decode(errors="replace")
    parts = line.split()
    verdict = parts[0]
    cycles = int(parts[1])
    us = int(parts[2])
    return verdict, cycles, us


def summarise(label: str, samples: list[int]) -> None:
    mean = statistics.mean(samples)
    stdev = statistics.stdev(samples) if len(samples) > 1 else 0.0
    lo = min(samples)
    hi = max(samples)
    cv = stdev / mean * 100 if mean else 0.0
    print(
        f"  {label:30s}  n={len(samples):5d}  mean={mean:8.1f} µs  "
        f"std={stdev:7.2f} µs  cv={cv:.3f}%  [{lo}..{hi}]"
    )


# Committed test vector: square circuit proof (x^2 = y, y public).
# Embedded so this script runs standalone on the Pi without needing the repo.
VALID_PROOF = (
    b")v\xbevB\x8fF'\x1a\xd0\x06\xf9\xec\xf4O\xbfRU\xab\x88\xe2\x99\x12 "
    b"%dT\x87 nff\x0e\xebJ\xccN\xad\xe5\xf1\xdb\xc8ET\x8f5f\x943n+F\x8d"
    b"\xf1\x86\xe3\x10\xe4-\xb7\xc5\x8c\x14U\x1fO\xcaQq'j\x19\x1fj\xffn"
    b"\xdcN\x8d\x1d\x98\xa2\x05\xa7\xca\xa2$:N\xef\xd9tg\xbal\xdb(\x85\xc9"
    b"\x08\xc2\xd7\x01S1\xc9\xed\xdd\x81\x92\xb4\x99\x10\xd7\x0c\n\xe3%"
    b"\xdf\x18\x1f\x1e\n\xfez3J\xb1\x0f\xef\xb9\xfex\xe2\xbf\xfb|\xc6\xc2"
    b"y\xa2Q\xf9n#\x87\xd6\x9cLa\xb9\xa2`\x1fs\xe0\x1b[*\xf8,\xe2\xc6@"
    b"\xd6\x9cq\xe2l\xd3W\xc70\xae\x15m\xb5\xcd\x15\xafZw\x13\x9a\xfe\xf2"
    b"\x81a\xc1L66\x01\xdb<,\x07d\xdb\xb7\xc3\xf3V\xd4\xa7\xf1\xd5\xa9\x14"
    b"\xab\x171W\x87|F\x0f]\x89\xfe\xca}\xda\xaf\x13\x19,\x8b\xb2K\x84Y\x19"
    b"\xc4\xd8I\xf2\x1d\xe4\xc4\xd0z7J\x93<\xc6Xg\xfb4\x8c\xf9\xf5\xba}"
)
assert len(VALID_PROOF) == PROOF_SIZE, (
    f"proof constant wrong length: {len(VALID_PROOF)}"
)


def run(args: argparse.Namespace) -> None:
    valid_proof = VALID_PROOF

    print(f"Opening {args.port} ...")
    port = serial.Serial(
        args.port, baudrate=115200, timeout=3, dsrdtr=False, rtscts=False
    )
    print("Syncing with device (sending probe) ...")
    sync_device(port)
    print(f"Device responding. Running {args.samples} samples per category.\n")

    categories: list[tuple[str, bytes]] = []

    # Valid proof this should always return T
    categories.append(("valid", valid_proof))

    # Bit-flip at byte 0 (first G1 x-coord — hits curve-check fast)
    flip0 = bytearray(valid_proof)
    flip0[0] ^= 0x01
    categories.append(("bit_flip_byte_0", bytes(flip0)))

    # Bit-flip at byte 127 (middle of B G2 point — likely verify-time failure)
    flip127 = bytearray(valid_proof)
    flip127[127] ^= 0x80
    categories.append(("bit_flip_byte_127", bytes(flip127)))

    # Bit-flip at byte 255 (last byte of C G1 y-coord)
    flip255 = bytearray(valid_proof)
    flip255[255] ^= 0x01
    categories.append(("bit_flip_byte_255", bytes(flip255)))

    # All-zeros, parse should fail fast on the identity point check
    categories.append(("all_zeros", bytes(PROOF_SIZE)))

    # Random bytes, almost certainly a parse or verify failure
    rng = random.Random(0xDEADBEEF)
    categories.append(
        ("random_bytes", bytes(rng.getrandbits(8) for _ in range(PROOF_SIZE)))
    )

    results: dict[str, dict] = {}

    for label, proof in categories:
        timings: list[int] = []
        verdicts: dict[str, int] = {}
        print(f"  collecting {label} ...", end="", flush=True)
        for i in range(args.samples):
            v, _cycles, us = query(port, proof)
            timings.append(us)
            verdicts[v] = verdicts.get(v, 0) + 1
            if (i + 1) % 100 == 0:
                print(f" {i + 1}", end="", flush=True)
        print()
        results[label] = {"timings": timings, "verdicts": verdicts}

    port.close()

    print("Results (µs round-trip, measured on-device):\n")
    print(
        f"  {'category':30s}  {'n':>7}  {'mean':>12}  {'std':>12}  {'cv':>9}  [min..max]"
    )
    print("  " + "-" * 90)

    for label, data in results.items():
        timings = data["timings"]
        verdicts = data["verdicts"]
        v_str = " ".join(f"{k}={n}" for k, n in sorted(verdicts.items()))
        mean = statistics.mean(timings)
        stdev = statistics.stdev(timings) if len(timings) > 1 else 0.0
        lo = min(timings)
        hi = max(timings)
        cv = stdev / mean * 100 if mean else 0.0
        print(
            f"  {label:30s}  n={len(timings):5d}  mean={mean:8.1f} µs  "
            f"std={stdev:7.2f} µs  cv={cv:.3f}%  [{lo}..{hi}]  ({v_str})"
        )

    # Timing oracle verdict: compare max of non-valid categories against valid mean.
    valid_mean = statistics.mean(results["valid"]["timings"])
    print("\nTiming oracle analysis:")
    print(f"  baseline (valid mean):  {valid_mean:.1f} µs")
    for label, data in results.items():
        if label == "valid":
            continue
        other_mean = statistics.mean(data["timings"])
        delta = other_mean - valid_mean
        pct = delta / valid_mean * 100 if valid_mean else 0.0
        print(f"  {label:30s}  delta={delta:+.1f} µs  ({pct:+.2f}%)")

    print("\nIf delta values are within noise (say < 2× stdev of valid), the verifier")
    print("does not leak distinguishable timing information over this transport.")


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--port", default="/dev/ttyACM0", help="USB serial port (default: /dev/ttyACM0)"
    )
    parser.add_argument(
        "--samples", type=int, default=1000, help="samples per category (default: 1000)"
    )
    args = parser.parse_args()
    try:
        run(args)
    except KeyboardInterrupt:
        print("\nAborted.")
        sys.exit(1)


if __name__ == "__main__":
    main()
