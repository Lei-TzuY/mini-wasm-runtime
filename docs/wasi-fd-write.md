# WASI Preview1 `fd_write`

`wasm-wasi` provides the first bounded WASI Preview1 host capability for the runtime. The crate does not embed a second WebAssembly engine and does not install capabilities implicitly; embedders explicitly register the adapter into an existing `HostRegistry`.

## Supported ABI

The current slice registers only:

```text
wasi_snapshot_preview1.fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> errno: i32
```

File descriptors `1` and `2` route to deterministic in-memory stdout and stderr buffers. Other descriptors return `ERRNO_BADF`.

The adapter decodes Preview1 `ciovec` entries as little-endian `{ pointer: u32, length: u32 }` pairs from guest memory, gathers the referenced byte ranges, writes the total byte count to `nwritten`, and returns `ERRNO_SUCCESS`.

## Fail-closed and resource semantics

The host callback uses `HostCapabilities::MEMORY_READ_WRITE`; it has no access to the `Instance` or unrelated host facilities. Every iovec header, payload, and the `nwritten` destination is checked before output bytes are committed. Guest-memory failures return `ERRNO_FAULT` and leave captured output unchanged.

The adapter also bounds the number of iovecs and aggregate bytes processed by one call. Inputs above those limits return `ERRNO_INVAL`, preventing one host call from becoming an unbounded work or allocation bypass around interpreter fuel accounting.

`OutputBuffer` exposes deterministic `snapshot`, `take`, and `clear` operations so embedders and tests can observe output without granting filesystem or terminal access.

## Deliberate boundary

This vertical slice does not provide `proc_exit`, arguments, environment variables, clocks, random sources, preopened directories, filesystem operations, sockets, or automatic `_start` execution. Those capabilities require separate threat-model and resource-accounting decisions and should land as independent executable slices.
