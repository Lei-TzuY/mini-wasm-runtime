//! Bounded WASI Preview1 host capabilities for `mini-wasm-runtime`.
//!
//! This crate provides executable descriptor capabilities on the runtime's
//! capability-scoped host boundary. Guest-memory operations are bounded and
//! fail closed before externally visible output side effects are committed.

use std::{cell::RefCell, rc::Rc};
use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

pub const ERRNO_SUCCESS: i32 = 0;
pub const ERRNO_BADF: i32 = 8;
pub const ERRNO_FAULT: i32 = 21;
pub const ERRNO_INVAL: i32 = 28;

pub const FILETYPE_CHARACTER_DEVICE: u8 = 2;
pub const RIGHTS_FD_WRITE: u64 = 1 << 6;

const FDSTAT_SIZE: usize = 24;
const DEFAULT_MAX_IOVECS: u32 = 1_024;
const DEFAULT_MAX_WRITE_BYTES: usize = 16 * 1024 * 1024;

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

#[derive(Debug, Clone)]
pub struct WasiPreview1 {
    stdout: OutputBuffer,
    stderr: OutputBuffer,
    max_iovecs: u32,
    max_write_bytes: usize,
}

impl Default for WasiPreview1 {
    fn default() -> Self {
        Self {
            stdout: OutputBuffer::default(),
            stderr: OutputBuffer::default(),
            max_iovecs: DEFAULT_MAX_IOVECS,
            max_write_bytes: DEFAULT_MAX_WRITE_BYTES,
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

                if !matches!(*fd, 1 | 2) {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                }

                let mut bytes = [0u8; FDSTAT_SIZE];
                bytes[0] = FILETYPE_CHARACTER_DEVICE;
                bytes[8..16].copy_from_slice(&RIGHTS_FD_WRITE.to_le_bytes());

                if context.write_memory(*fdstat as u32, &bytes).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )
    }
}