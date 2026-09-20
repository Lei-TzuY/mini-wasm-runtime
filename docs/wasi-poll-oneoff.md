# Bounded WASI Preview1 `poll_oneoff`

This slice adds a deliberately bounded clock-only `poll_oneoff` capability to the existing deterministic WASI Preview1 adapter.

## Executable contract

The adapter decodes Preview1 32-bit guest ABI subscriptions and emits Preview1 events for clock subscriptions that are already ready at the injected clock snapshot:

- relative clock subscriptions with a zero timeout;
- absolute realtime or monotonic subscriptions whose deadline is less than or equal to the configured deterministic snapshot.

The implementation preserves each subscription's `userdata`, returns `ERRNO_SUCCESS` in the event, emits `eventtype::clock`, writes the final event count, and supports up to 64 subscriptions per call.

All guest input, output, and `nevents` ranges are preflighted before any output mutation. Invalid clock IDs or flags return `ERRNO_INVAL`; guest-memory faults return `ERRNO_FAULT`.

## Deliberate fail-closed boundary

This runtime does not yet have a scheduler or a clock provider that can advance while a host call blocks. Therefore:

- future relative timeouts;
- future absolute deadlines; and
- fd-read/fd-write readiness subscriptions

return `ERRNO_NOTSUP` without partially writing output state.

This is intentional. The slice does not claim blocking sleep semantics, descriptor readiness polling, sockets, or an asynchronous event loop.

## Evidence

Focused runtime regressions lock:

- exact Preview1 subscription/event ABI layout;
- multiple ready clock subscriptions;
- relative-zero and expired-absolute readiness;
- fail-closed future timers;
- invalid clock IDs/flags;
- zero-subscription rejection; and
- atomic out-of-bounds handling.

The isolated differential workspace executes the same two ready monotonic subscriptions in the mini runtime and pinned Wasmtime-WASI 37.0.3, comparing errno, event count, userdata, event type, event errno, and fd-read/write payload fields while intentionally ignoring ABI padding bytes.
