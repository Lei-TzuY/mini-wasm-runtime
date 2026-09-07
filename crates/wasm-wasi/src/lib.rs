//! Bounded WASI Preview1 host capabilities for `mini-wasm-runtime`.
//!
//! This crate provides executable descriptor I/O, process-argument, and process-environment
//! capabilities on the runtime's capability-scoped host boundary. Guest-memory operations are
//! bounded and fail closed before externally visible I/O side effects are committed.

use std::{cell::RefCell, rc::Rc};
use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

pub const ERRNO_SUCCESS: i32 = 0;
pub const ERRNO_BADF: i32 = 8;
pub const ERRNO_FAULT: i32 = 21;
pub const ERRNO_INVAL: i32 = 28;

pub const FILETYPE_CHARACTER_DEVICE: u8 = 2;
pub const RIGHTS_FD_READ: u64 = 1 << 1;
pub const RIGHTS_FD_WRITE: u64 = 1 << 6;

const FDSTAT_SIZE: usize = 24;
const DEFAULT_MAX_IOVECS: u32 = 1_024;
const DEFAULT_MAX_READ_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_WRITE_BYTES: usize = 16 * 1024 * 1024;
const DEFAULT_MAX_ARGS: usize = 4_096;
const DEFAULT_MAX_ARGS_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_ENV: usize = 4_096;
const DEFAULT_MAX_ENV_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct OutputBuffer {
    bytes: Rc<RefCell<Vec<u8>>>,
}

impl OutputBuffer {
    pub fn snapshot(&self) -> Vec<u8> {
        self.bytes.borrow().clone()
    }

    pub fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.bytes.borrow_mut())
    }

    pub fn clear(&self) {
        self.bytes.borrow_mut().clear();
    }

    fn append(&self, bytes: &[u8]) {
        self.bytes.borrow_mut().extend_from_slice(bytes);
    }
}

#[derive(Debug, Default)]
struct InputState {
    bytes: Vec<u8>,
    offset: usize,
}

#[derive(Debug, Clone, Default)]
struct InputBuffer {
    state: Rc<RefCell<InputState>>,
}

impl InputBuffer {
    fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            state: Rc::new(RefCell::new(InputState {
                bytes: bytes.to_vec(),
                offset: 0,
            })),
        }
    }

    fn peek(&self, max_len: usize) -> Vec<u8> {
        let state = self.state.borrow();
        let remaining = state.bytes.len().saturating_sub(state.offset);
        let len = remaining.min(max_len);
        state.bytes[state.offset..state.offset + len].to_vec()
    }

    fn advance(&self, len: usize) {
        let mut state = self.state.borrow_mut();
        state.offset = state.offset.saturating_add(len).min(state.bytes.len());
    }
}

#[derive(Debug, Clone)]
pub struct WasiPreview1 {
    stdin: InputBuffer,
    stdout: OutputBuffer,
    stderr: OutputBuffer,
    args: Vec<Vec<u8>>,
    env: Vec<Vec<u8>>,
    max_iovecs: u32,
    max_write_bytes: usize,
    max_read_iovecs: u32,
    max_read_bytes: usize,
    max_args: usize,
    max_args_bytes: usize,
    max_env: usize,
    max_env_bytes: usize,
}

impl Default for WasiPreview1 {
    fn default() -> Self {
        Self {
            stdin: InputBuffer::default(),
            stdout: OutputBuffer::default(),
            stderr: OutputBuffer::default(),
            args: Vec::new(),
            env: Vec::new(),
            max_iovecs: DEFAULT_MAX_IOVECS,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
            max_read_iovecs: DEFAULT_MAX_IOVECS,
            max_read_bytes: DEFAULT_MAX_READ_BYTES,
            max_args: DEFAULT_MAX_ARGS,
            max_args_bytes: DEFAULT_MAX_ARGS_BYTES,
            max_env: DEFAULT_MAX_ENV,
            max_env_bytes: DEFAULT_MAX_ENV_BYTES,
        }
    }
}

impl WasiPreview1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stdout(&self) -> OutputBuffer {
        self.stdout.clone()
    }

    pub fn stderr(&self) -> OutputBuffer {
        self.stderr.clone()
    }

    pub fn with_limits(mut self, max_iovecs: u32, max_write_bytes: usize) -> Self {
        self.max_iovecs = max_iovecs;
        self.max_write_bytes = max_write_bytes;
        self
    }

    pub fn with_stdin<B: AsRef<[u8]>>(mut self, bytes: B) -> Self {
        self.stdin = InputBuffer::from_bytes(bytes.as_ref());
        self
    }

    pub fn with_read_limits(mut self, max_iovecs: u32, max_read_bytes: usize) -> Self {
        self.max_read_iovecs = max_iovecs;
        self.max_read_bytes = max_read_bytes;
        self
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args = args
            .into_iter()
            .map(|arg| arg.as_ref().as_bytes().to_vec())
            .collect();
        self
    }

    pub fn with_args_limits(mut self, max_args: usize, max_args_bytes: usize) -> Self {
        self.max_args = max_args;
        self.max_args_bytes = max_args_bytes;
        self
    }

    pub fn with_env<I, K, V>(mut self, env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.env = env
            .into_iter()
            .map(|(key, value)| {
                let key = key.as_ref().as_bytes();
                let value = value.as_ref().as_bytes();
                let mut entry =
                    Vec::with_capacity(key.len().saturating_add(value.len()).saturating_add(1));
                entry.extend_from_slice(key);
                entry.push(b'=');
                entry.extend_from_slice(value);
                entry
            })
            .collect();
        self
    }

    pub fn with_env_limits(mut self, max_env: usize, max_env_bytes: usize) -> Self {
        self.max_env = max_env;
        self.max_env_bytes = max_env_bytes;
        self
    }

    pub fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        let stdout = self.stdout.clone();
        let stderr = self.stderr.clone();
        let max_iovecs = self.max_iovecs;
        let max_write_bytes = self.max_write_bytes;

        registry.register_values(
            "wasi_snapshot_preview1",
            "fd_write",
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(iovs), Value::I32(iovs_len), Value::I32(nwritten)] =
                    args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_write signature received non-i32 arguments",
                    ));
                };

                let output = match *fd {
                    1 => &stdout,
                    2 => &stderr,
                    _ => return Ok(vec![Value::I32(ERRNO_BADF)]),
                };

                let iovs_len = *iovs_len as u32;
                if iovs_len > max_iovecs {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                let mut chunks = Vec::with_capacity(iovs_len as usize);
                let mut total = 0usize;
                for index in 0..iovs_len {
                    let Some(entry_offset) = index.checked_mul(8) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let Some(entry_address) = (*iovs as u32).checked_add(entry_offset) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let header = match context.read_memory(entry_address, 8) {
                        Ok(header) => header,
                        Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                    };
                    let pointer =
                        u32::from_le_bytes(header[0..4].try_into().expect("fixed iovec header"));
                    let length =
                        u32::from_le_bytes(header[4..8].try_into().expect("fixed iovec header"));
                    let length = length as usize;
                    let Some(next_total) = total.checked_add(length) else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    if next_total > max_write_bytes || next_total > u32::MAX as usize {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    }
                    let bytes = match context.read_memory(pointer, length) {
                        Ok(bytes) => bytes,
                        Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                    };
                    total = next_total;
                    chunks.push(bytes);
                }

                if context.read_memory(*nwritten as u32, 4).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let total = total as u32;
                if context
                    .write_memory(*nwritten as u32, &total.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                for chunk in chunks {
                    output.append(&chunk);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let stdin = self.stdin.clone();
        let max_read_iovecs = self.max_read_iovecs;
        let max_read_bytes = self.max_read_bytes;
        registry.register_values(
            "wasi_snapshot_preview1",
            "fd_read",
            vec![
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
                ValueType::I32,
            ],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(iovs), Value::I32(iovs_len), Value::I32(nread)] =
                    args
                else {
                    return Err(HostError::message(
                        "validated wasi fd_read signature received non-i32 arguments",
                    ));
                };

                if *fd != 0 {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }

                let iovs_len = *iovs_len as u32;
                if iovs_len > max_read_iovecs {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                let mut destinations = Vec::with_capacity(iovs_len as usize);
                let mut total_capacity = 0usize;
                for index in 0..iovs_len {
                    let Some(entry_offset) = index.checked_mul(8) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let Some(entry_address) = (*iovs as u32).checked_add(entry_offset) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    let header = match context.read_memory(entry_address, 8) {
                        Ok(header) => header,
                        Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
                    };
                    let pointer =
                        u32::from_le_bytes(header[0..4].try_into().expect("fixed iovec header"));
                    let length =
                        u32::from_le_bytes(header[4..8].try_into().expect("fixed iovec header"))
                            as usize;
                    let Some(next_total) = total_capacity.checked_add(length) else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    if next_total > max_read_bytes || next_total > u32::MAX as usize {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    }
                    if context.read_memory(pointer, length).is_err() {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    }
                    total_capacity = next_total;
                    destinations.push((pointer, length));
                }

                if context.read_memory(*nread as u32, 4).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let bytes = stdin.peek(total_capacity);
                let mut copied = 0usize;
                for (pointer, length) in destinations {
                    if copied == bytes.len() {
                        break;
                    }
                    let chunk_len = length.min(bytes.len() - copied);
                    if chunk_len != 0
                        && context
                            .write_memory(pointer, &bytes[copied..copied + chunk_len])
                            .is_err()
                    {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    }
                    copied += chunk_len;
                }

                let copied_u32 = copied as u32;
                if context
                    .write_memory(*nread as u32, &copied_u32.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                stdin.advance(copied);

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        registry.register_values(
            "wasi_snapshot_preview1",
            "fd_fdstat_get",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(fdstat)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_fdstat_get signature received non-i32 arguments",
                    ));
                };

                let rights = match *fd {
                    0 => RIGHTS_FD_READ,
                    1 | 2 => RIGHTS_FD_WRITE,
                    _ => return Ok(vec![Value::I32(ERRNO_BADF)]),
                };

                let mut bytes = [0u8; FDSTAT_SIZE];
                bytes[0] = FILETYPE_CHARACTER_DEVICE;
                bytes[8..16].copy_from_slice(&rights.to_le_bytes());

                if context.write_memory(*fdstat as u32, &bytes).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let configured_args = self.args.clone();
        let max_args = self.max_args;
        let max_args_bytes = self.max_args_bytes;
        let sizes_args = configured_args.clone();
        registry.register_values(
            "wasi_snapshot_preview1",
            "args_sizes_get",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(argc_ptr), Value::I32(argv_buf_size_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi args_sizes_get signature received non-i32 arguments",
                    ));
                };

                if sizes_args.len() > max_args {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                let mut bytes_len = 0usize;
                for arg in &sizes_args {
                    let Some(next) = bytes_len
                        .checked_add(arg.len())
                        .and_then(|n| n.checked_add(1))
                    else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    bytes_len = next;
                }
                if bytes_len > max_args_bytes
                    || sizes_args.len() > u32::MAX as usize
                    || bytes_len > u32::MAX as usize
                {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                if context.read_memory(*argc_ptr as u32, 4).is_err()
                    || context.read_memory(*argv_buf_size_ptr as u32, 4).is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let argc = (sizes_args.len() as u32).to_le_bytes();
                let argv_buf_size = (bytes_len as u32).to_le_bytes();
                if context.write_memory(*argc_ptr as u32, &argc).is_err()
                    || context
                        .write_memory(*argv_buf_size_ptr as u32, &argv_buf_size)
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        registry.register_values(
            "wasi_snapshot_preview1",
            "args_get",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(argv_ptr), Value::I32(argv_buf_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi args_get signature received non-i32 arguments",
                    ));
                };

                if configured_args.len() > max_args {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                let Some(pointer_bytes_len) = configured_args.len().checked_mul(4) else {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                };
                let mut payload = Vec::new();
                let mut pointer_table = Vec::with_capacity(pointer_bytes_len);
                for arg in &configured_args {
                    let Some(offset) = u32::try_from(payload.len()).ok() else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    let Some(pointer) = (*argv_buf_ptr as u32).checked_add(offset) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    pointer_table.extend(pointer.to_le_bytes());
                    let Some(next_len) = payload
                        .len()
                        .checked_add(arg.len())
                        .and_then(|n| n.checked_add(1))
                    else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    if next_len > max_args_bytes || next_len > u32::MAX as usize {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    }
                    payload.extend_from_slice(arg);
                    payload.push(0);
                }

                if context
                    .read_memory(*argv_ptr as u32, pointer_table.len())
                    .is_err()
                    || context
                        .read_memory(*argv_buf_ptr as u32, payload.len())
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                if context
                    .write_memory(*argv_ptr as u32, &pointer_table)
                    .is_err()
                    || context
                        .write_memory(*argv_buf_ptr as u32, &payload)
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let configured_env = self.env.clone();
        let max_env = self.max_env;
        let max_env_bytes = self.max_env_bytes;
        let sizes_env = configured_env.clone();
        registry.register_values(
            "wasi_snapshot_preview1",
            "environ_sizes_get",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(count_ptr), Value::I32(buf_size_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi environ_sizes_get signature received non-i32 arguments",
                    ));
                };

                if sizes_env.len() > max_env {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                let mut bytes_len = 0usize;
                for entry in &sizes_env {
                    let Some(next) = bytes_len
                        .checked_add(entry.len())
                        .and_then(|n| n.checked_add(1))
                    else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    bytes_len = next;
                }
                if bytes_len > max_env_bytes
                    || sizes_env.len() > u32::MAX as usize
                    || bytes_len > u32::MAX as usize
                {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                if context.read_memory(*count_ptr as u32, 4).is_err()
                    || context.read_memory(*buf_size_ptr as u32, 4).is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                let count = (sizes_env.len() as u32).to_le_bytes();
                let buf_size = (bytes_len as u32).to_le_bytes();
                if context.write_memory(*count_ptr as u32, &count).is_err()
                    || context
                        .write_memory(*buf_size_ptr as u32, &buf_size)
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        registry.register_values(
            "wasi_snapshot_preview1",
            "environ_get",
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(environ_ptr), Value::I32(environ_buf_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi environ_get signature received non-i32 arguments",
                    ));
                };

                if configured_env.len() > max_env {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }
                let Some(pointer_bytes_len) = configured_env.len().checked_mul(4) else {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                };
                let mut payload = Vec::new();
                let mut pointer_table = Vec::with_capacity(pointer_bytes_len);
                for entry in &configured_env {
                    let Some(offset) = u32::try_from(payload.len()).ok() else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    let Some(pointer) = (*environ_buf_ptr as u32).checked_add(offset) else {
                        return Ok(vec![Value::I32(ERRNO_FAULT)]);
                    };
                    pointer_table.extend(pointer.to_le_bytes());
                    let Some(next_len) = payload
                        .len()
                        .checked_add(entry.len())
                        .and_then(|n| n.checked_add(1))
                    else {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    };
                    if next_len > max_env_bytes || next_len > u32::MAX as usize {
                        return Ok(vec![Value::I32(ERRNO_INVAL)]);
                    }
                    payload.extend_from_slice(entry);
                    payload.push(0);
                }

                if context
                    .read_memory(*environ_ptr as u32, pointer_table.len())
                    .is_err()
                    || context
                        .read_memory(*environ_buf_ptr as u32, payload.len())
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                if context
                    .write_memory(*environ_ptr as u32, &pointer_table)
                    .is_err()
                    || context
                        .write_memory(*environ_buf_ptr as u32, &payload)
                        .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )
    }
}
