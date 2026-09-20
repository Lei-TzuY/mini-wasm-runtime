# Bounded WASI Preview1 `poll_oneoff`

This surface provides a deliberately bounded `poll_oneoff` capability for events whose readiness can be decided synchronously from deterministic runtime state.

## Executable contract

The adapter decodes Preview1 32-bit guest ABI subscriptions and emits Preview1 events for:

- relative clock subscriptions with a zero timeout;
- absolute realtime or monotonic subscriptions whose deadline is less than or equal to the configured deterministic snapshot;
- injected stdin read readiness;
- stdout/stderr write readiness; and
- opened regular-file read/write readiness when descriptor rights permit the requested direction.

Regular-file read readiness follows the live descriptor cursor. It reports the Preview1 read/write HANGUP flag at EOF, while preserving Wasmtime-WASI 37.0.3's `nbytes = 1` compatibility behavior. File-write and stdio readiness likewise use the pinned reference behavior of `nbytes = 1`.

The implementation preserves each subscription's `userdata`, emits the matching event type, writes the final event count, and supports up to 64 subscriptions per call.

All guest input, output, and `nevents` ranges are preflighted before any output mutation. Invalid clock IDs or flags return `ERRNO_INVAL`; guest-memory faults return `ERRNO_FAULT`.

## Deliberate fail-closed boundary

This runtime does not yet have a scheduler or a clock provider that can advance while a host call blocks. Therefore future relative timeouts and future absolute deadlines return `ERRNO_NOTSUP` without partially writing output state.

Descriptor polling is intentionally limited to injected stdio and in-memory regular-file descriptors whose readiness is immediately knowable. Unsupported descriptor kinds such as directories or sockets fail closed rather than pretending to provide asynchronous readiness.

This surface does not claim blocking sleep semantics, sockets, host-kernel polling, or an asynchronous event loop.

## Evidence

Focused runtime regressions lock:

- exact Preview1 subscription/event ABI layout;
- multiple ready clock subscriptions;
- relative-zero and expired-absolute readiness;
- fail-closed future timers;
- immediate stdin/stdout readiness;
- invalid or direction-incompatible descriptors with no partial output;
- invalid clock IDs/flags;
- zero-subscription rejection; and
- atomic out-of-bounds handling.

The isolated differential workspace covers both clock and descriptor readiness against pinned Wasmtime-WASI 37.0.3. The regular-file trace compares dynamic descriptor allocation, read/write event ordering, userdata, event errno/type, `nbytes`, and the EOF HANGUP transition while intentionally ignoring ABI padding bytes.
