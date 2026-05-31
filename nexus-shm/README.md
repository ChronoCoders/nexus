# nexus-shm

Shared-memory IPC primitives for multi-process trading systems.

> **Status: foundation in progress.** The mmap lifecycle, two-tier liveness,
> and the primitives below are under active development on top of the
> foundation layer that exists today (`Pod`, the segment control block). Track
> progress in [#390](https://github.com/Abso1ut3Zer0/nexus/issues/390).

## Overview

`nexus-shm` extends nexus's single-process primitives across the process
boundary: cross-process messaging and durable storage over memory-mapped files,
with the same principles — bounded, pre-allocated, cache-line aware, and honest
about its failure modes.

The ring buffer is the easy part; nexus-queue already solves that. The hard
parts are what live here:

- **Liveness** — telling a dead peer from a busy one without blocking your side.
- **Crash recovery** — a `SIGKILL` leaves half-written state in shared memory;
  there is no unwinding and no RAII cleanup on the far side.
- **Memory lifecycle** — who creates the mapping, who attaches, what happens
  when one side restarts.

These problems shape the memory layout, so they are designed first.

## Primitives

| Primitive | Role | Tracking |
|-----------|------|----------|
| `ShmJournal` | Append-only durable log (FIX journaling, replay) | [#391](https://github.com/Abso1ut3Zer0/nexus/issues/391) |
| `ShmRingBuffer` | Cross-process SPSC ring buffer (market-data fan-out) | [#392](https://github.com/Abso1ut3Zer0/nexus/issues/392) |
| `ShmSlot` | Cross-process seqlock slot (latest-value snapshots) | [#393](https://github.com/Abso1ut3Zer0/nexus/issues/393) |

All three sit on a common mmap foundation: get it right once, everything above
inherits it.

## Liveness — two-tier

A reader cannot trust a peer to be alive just because a shared atomic says so.
Under `panic=abort`, `SIGKILL`, or a segfault no `Drop` runs, so the atomic
stays `ALIVE` forever. Timing-based heartbeats can't separate a dead peer from a
busy one either. So liveness is split into a fast hint and a definitive source
of truth:

| Tier | Mechanism | Cost | Catches | Misses |
|------|-----------|------|---------|--------|
| **1** | Atomic status field (`ALIVE`/`DEAD` via `Drop` guard) | ~1 cycle | Graceful shutdown, unwinding panic | `panic=abort`, `SIGKILL`, segfault |
| **2** | OFD lock (`fcntl`), kernel-released on death | a syscall | *Any* process death, including `SIGKILL` | — |

Tier 1 is the hot-path hint; **Tier 2 is the source of truth**. Under
`panic=abort`, Tier 2 is mandatory, not optional. For in-process shared memory
(same process reads and writes — e.g. a FIX journal), Tier 1 alone suffices:
process death takes out both ends, so no surviving peer can be misled.

Detection policy stays with the caller. The library exposes a liveness query
but never calls it on the hot path — the OFD-lock syscall is too costly to run
per message, so the caller decides when (on attach, on a slow timer, on a
suspected stall). A **generation counter** in the control block ties the tiers
together: an attacher that sees `ALIVE` but finds the kernel lock free knows the
previous owner died hard, bumps the generation, and runs recovery. That
staleness signal is structural, not timing-based, and stays unambiguous across
PID reuse.

## The `Pod` Trait

Every type stored in shared memory must be `Pod` (Plain Old Data): a flat value
with a stable `repr`-defined layout, no pointers, and no `Drop` glue. The bytes
*are* the value, so they stay meaningful when byte-copied between processes that
mapped the segment at different addresses.

```rust
use nexus_shm::Pod;

#[repr(C)]
#[derive(Clone, Copy)]
struct BookSnapshot {
    bids: [f64; 20],
    asks: [f64; 20],
    sequence: u64,
}

// SAFETY: flat repr(C), no pointers, no Drop, valid for every bit pattern.
unsafe impl Pod for BookSnapshot {}
```

This is stricter than nexus-slot's `Pod`. A reader may observe bytes mid-write
or from a crashed writer, so a `Pod` type must be **valid for every bit pattern
of its size**. That is why `bool` and `char` are *not* `Pod` here even though
they are `Copy`: most bit patterns are invalid for them, and observing one
across the process boundary would be instant UB. Integers, floats, and arrays
of `Pod` are covered; compose your own types from those.

## Design

The first bytes of every segment are a fixed control block carrying only
*universal* metadata — identity, the two liveness fields, and the payload
length:

```text
offset 0  ┌──────────────────────────────────────────────┐
          │ ControlBlock  (CachePadded — owns its line)   │
          │   magic · layout_ver · flags                  │  identity
          │   generation · status · owner_pid             │  liveness
          │   data_len                                    │
          ├──────────────────────────────────────────────┤
          │ Per-primitive header  (own cache line)        │  ring head/tail,
          │   ...                                         │  slot sequence, ...
          ├──────────────────────────────────────────────┤
          │ Payload                                       │
          └──────────────────────────────────────────────┘
```

The control block is payload-agnostic. Per-primitive metadata lives in each
primitive's own header on its own cache line, so hot per-message state (a ring's
head/tail, a slot's sequence counter) never false-shares with the rarely-written
control fields. `CachePadded` keeps this portable across cache-line sizes
(64-byte on x86-64, 128-byte on some aarch64).

## References

- **Aeron** — log-buffer design, archive model, commit-marker recovery.
- **iceoryx2** — `fcntl` advisory locks in production (validates Tier 2).
- **nexus-queue / nexus-slot** — single-process patterns this mirrors
  (cache-line padding, manual fencing, the seqlock).

## License

MIT OR Apache-2.0
