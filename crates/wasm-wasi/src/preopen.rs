use std::fmt;

use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::{ERRNO_BADF, ERRNO_FAULT, ERRNO_NAMETOOLONG, ERRNO_SUCCESS};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const FD_PRESTAT_GET_NAME: &str = "fd_prestat_get";
const FD_PRESTAT_DIR_NAME_NAME: &str = "fd_prestat_dir_name";
const PREOPENTYPE_DIR: u8 = 0;
const PRESTAT_SIZE: usize = 8;
const FIRST_PREOPEN_FD: u32 = 3;
const MAX_PREOPEN_DIRS: usize = 128;
const MAX_PREOPEN_NAME_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WasiPreopenError {
    EmptyGuestPath,
    NameTooLong { length: usize, limit: usize },
    TooManyPreopens { limit: usize },
    DescriptorOverflow,
}

impl fmt::Display for WasiPreopenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyGuestPath => write!(f, "WASI preopen guest path must not be empty"),
            Self::NameTooLong { length, limit } => write!(
                f,
                "WASI preopen guest path is {length} bytes, exceeding the {limit}-byte limit"
            ),
            Self::TooManyPreopens { limit } => {
                write!(f, "WASI preopen count exceeds the fixed limit of {limit}")
            }
            Self::DescriptorOverflow => write!(f, "WASI preopen descriptor allocation overflowed"),
        }
    }
}

impl std::error::Error for WasiPreopenError {}

#[derive(Debug, Clone)]
struct PreopenDir {
    fd: u32,
    guest_path: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct PreopenSet {
    entries: Vec<PreopenDir>,
}

impl PreopenSet {
    pub(crate) fn add(&mut self, guest_path: &[u8]) -> Result<u32, WasiPreopenError> {
        if guest_path.is_empty() {
            return Err(WasiPreopenError::EmptyGuestPath);
        }
        if guest_path.len() > MAX_PREOPEN_NAME_BYTES {
            return Err(WasiPreopenError::NameTooLong {
                length: guest_path.len(),
                limit: MAX_PREOPEN_NAME_BYTES,
            });
        }
        if self.entries.len() >= MAX_PREOPEN_DIRS {
            return Err(WasiPreopenError::TooManyPreopens {
                limit: MAX_PREOPEN_DIRS,
            });
        }

        let index =
            u32::try_from(self.entries.len()).map_err(|_| WasiPreopenError::DescriptorOverflow)?;
        let fd = FIRST_PREOPEN_FD
            .checked_add(index)
            .ok_or(WasiPreopenError::DescriptorOverflow)?;
        self.entries.push(PreopenDir {
            fd,
            guest_path: guest_path.to_vec(),
        });
        Ok(fd)
    }

    fn find(&self, fd: i32) -> Option<&PreopenDir> {
        let fd = fd as u32;
        self.entries.iter().find(|entry| entry.fd == fd)
    }

    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        let prestats = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_PRESTAT_GET_NAME,
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(prestat_ptr)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_prestat_get signature received invalid arguments",
                    ));
                };

                let Some(entry) = prestats.find(*fd) else {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                };

                let mut bytes = [0u8; PRESTAT_SIZE];
                bytes[0] = PREOPENTYPE_DIR;
                bytes[4..8].copy_from_slice(&(entry.guest_path.len() as u32).to_le_bytes());
                let address = *prestat_ptr as u32;
                if context.read_memory(address, PRESTAT_SIZE).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context.write_memory(address, &bytes).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let names = self.clone();
        registry.register_values(
            WASI_MODULE,
            FD_PRESTAT_DIR_NAME_NAME,
            vec![ValueType::I32, ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(fd), Value::I32(path_ptr), Value::I32(path_len)] = args else {
                    return Err(HostError::message(
                        "validated wasi fd_prestat_dir_name signature received invalid arguments",
                    ));
                };

                let Some(entry) = names.find(*fd) else {
                    return Ok(vec![Value::I32(ERRNO_BADF)]);
                };
                let capacity = *path_len as u32 as usize;
                if capacity < entry.guest_path.len() {
                    return Ok(vec![Value::I32(ERRNO_NAMETOOLONG)]);
                }

                let address = *path_ptr as u32;
                if context
                    .read_memory(address, entry.guest_path.len())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context.write_memory(address, &entry.guest_path).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )
    }
}
