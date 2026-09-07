//! Bounded WASI Preview1 host capabilities for `mini-wasm-runtime`.
//!
//! Existing descriptor, argument, and environment capabilities remain isolated in the original
//! implementation module. This crate root layers typed process termination, deterministic
//! entropy/clocks, preopen discovery, and an injected read-only path capability over that API.

#[path = "lib.rs"]
mod base;
mod clock;
mod filesystem;
mod preopen;

pub use base::{
    OutputBuffer, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOTCAPABLE, ERRNO_SUCCESS,
    FILETYPE_CHARACTER_DEVICE, FILETYPE_DIRECTORY, FILETYPE_REGULAR_FILE, RIGHTS_FD_READ,
    RIGHTS_FD_WRITE, RIGHTS_PATH_OPEN,
};
pub use clock::WasiClockId;
pub use filesystem::WasiFilesystemError;
pub use preopen::WasiPreopenError;

use std::{cell::RefCell, rc::Rc};
use wasm_parser::ValueType;
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

pub const ERRNO_IO: i32 = 29;
pub const ERRNO_MFILE: i32 = 33;
pub const ERRNO_NAMETOOLONG: i32 = 37;
pub const ERRNO_NOENT: i32 = 44;

const PROC_EXIT_MODULE: &str = "wasi_snapshot_preview1";
const PROC_EXIT_NAME: &str = "proc_exit";
const PROC_EXIT_CONTROL_TRANSFER: &str = "wasi proc_exit control transfer";
const RANDOM_GET_NAME: &str = "random_get";
const DEFAULT_MAX_RANDOM_BYTES: usize = 1024 * 1024;

#[derive(Debug, Default)]
struct EntropyState {
    bytes: Vec<u8>,
    offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WasiInvocationOutcome {
    Returned(Vec<Value>),
    Exited(u32),
}

#[derive(Debug, Clone)]
pub struct WasiPreview1 {
    base: base::WasiPreview1,
    exit_code: Rc<RefCell<Option<u32>>>,
    entropy: Rc<RefCell<EntropyState>>,
    max_random_bytes: usize,
    clocks: clock::ClockSet,
    preopens: preopen::PreopenSet,
    filesystem: filesystem::Filesystem,
}

impl Default for WasiPreview1 {
    fn default() -> Self {
        let filesystem = filesystem::Filesystem::default();
        Self {
            base: base::WasiPreview1::new().with_filesystem(filesystem.clone()),
            exit_code: Rc::new(RefCell::new(None)),
            entropy: Rc::new(RefCell::new(EntropyState::default())),
            max_random_bytes: DEFAULT_MAX_RANDOM_BYTES,
            clocks: clock::ClockSet::default(),
            preopens: preopen::PreopenSet::default(),
            filesystem,
        }
    }
}

impl WasiPreview1 {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stdout(&self) -> OutputBuffer {
        self.base.stdout()
    }

    pub fn stderr(&self) -> OutputBuffer {
        self.base.stderr()
    }

    pub fn with_limits(mut self, max_iovecs: u32, max_write_bytes: usize) -> Self {
        self.base = self.base.with_limits(max_iovecs, max_write_bytes);
        self
    }

    pub fn with_stdin<B: AsRef<[u8]>>(mut self, bytes: B) -> Self {
        self.base = self.base.with_stdin(bytes);
        self
    }

    pub fn with_read_limits(mut self, max_iovecs: u32, max_read_bytes: usize) -> Self {
        self.base = self.base.with_read_limits(max_iovecs, max_read_bytes);
        self
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.base = self.base.with_args(args);
        self
    }

    pub fn with_args_limits(mut self, max_args: usize, max_args_bytes: usize) -> Self {
        self.base = self.base.with_args_limits(max_args, max_args_bytes);
        self
    }

    pub fn with_env<I, K, V>(mut self, env: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        self.base = self.base.with_env(env);
        self
    }

    pub fn with_env_limits(mut self, max_env: usize, max_env_bytes: usize) -> Self {
        self.base = self.base.with_env_limits(max_env, max_env_bytes);
        self
    }

    pub fn with_random_bytes<B: AsRef<[u8]>>(mut self, bytes: B) -> Self {
        self.entropy = Rc::new(RefCell::new(EntropyState {
            bytes: bytes.as_ref().to_vec(),
            offset: 0,
        }));
        self
    }

    pub fn with_random_limit(mut self, max_random_bytes: usize) -> Self {
        self.max_random_bytes = max_random_bytes;
        self
    }

    pub fn with_clock(mut self, id: WasiClockId, resolution_ns: u64, time_ns: u64) -> Self {
        self.clocks.configure(id, resolution_ns, time_ns);
        self
    }

    pub fn with_preopen<S: AsRef<str>>(mut self, guest_path: S) -> Result<Self, WasiPreopenError> {
        let fd = self.preopens.add(guest_path.as_ref().as_bytes())?;
        self.filesystem.reserve_preopen(fd);
        self.base = self.base.with_fdstat(
            fd,
            FILETYPE_DIRECTORY,
            RIGHTS_PATH_OPEN,
            RIGHTS_FD_READ,
        );
        Ok(self)
    }

    pub fn with_read_only_file<P, S, B>(
        self,
        preopen_guest_path: P,
        relative_path: S,
        bytes: B,
    ) -> Result<Self, WasiFilesystemError>
    where
        P: AsRef<str>,
        S: AsRef<str>,
        B: AsRef<[u8]>,
    {
        let guest_path = preopen_guest_path.as_ref();
        let Some(preopen_fd) = self
            .preopens
            .fd_for_guest_path(guest_path.as_bytes())
        else {
            return Err(WasiFilesystemError::UnknownPreopen {
                guest_path: guest_path.to_owned(),
            });
        };

        self.filesystem.mount_file(
            preopen_fd,
            relative_path.as_ref().as_bytes(),
            bytes.as_ref(),
        )?;
        Ok(self)
    }

    pub fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        self.base.register(registry)?;
        self.preopens.register(registry)?;
        self.filesystem.register(registry)?;

        let entropy = self.entropy.clone();
        let max_random_bytes = self.max_random_bytes;
        registry.register_values(
            PROC_EXIT_MODULE,
            RANDOM_GET_NAME,
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(buffer), Value::I32(buffer_len)] = args else {
                    return Err(HostError::message(
                        "validated wasi random_get signature received non-i32 arguments",
                    ));
                };

                let len = *buffer_len as u32 as usize;
                if len > max_random_bytes {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                }

                let bytes = {
                    let state = entropy.borrow();
                    let remaining = state.bytes.len().saturating_sub(state.offset);
                    if len > remaining {
                        return Ok(vec![Value::I32(ERRNO_IO)]);
                    }
                    state.bytes[state.offset..state.offset + len].to_vec()
                };

                if context.read_memory(*buffer as u32, len).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context.write_memory(*buffer as u32, &bytes).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                entropy.borrow_mut().offset += len;

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        self.clocks.register(registry)?;

        let exit_code = self.exit_code.clone();
        registry.register_values(
            PROC_EXIT_MODULE,
            PROC_EXIT_NAME,
            vec![ValueType::I32],
            vec![],
            HostCapabilities::NONE,
            move |_context, args| {
                let [Value::I32(code)] = args else {
                    return Err(HostError::message(
                        "validated wasi proc_exit signature received non-i32 arguments",
                    ));
                };

                *exit_code.borrow_mut() = Some(*code as u32);
                Err(HostError::message(PROC_EXIT_CONTROL_TRANSFER))
            },
        )
    }

    pub fn invoke_export_values(
        &self,
        instance: &mut Instance,
        name: &str,
        args: &[Value],
    ) -> Result<WasiInvocationOutcome, RuntimeError> {
        *self.exit_code.borrow_mut() = None;

        match instance.invoke_export_values(name, args) {
            Ok(values) => Ok(WasiInvocationOutcome::Returned(values)),
            Err(error) => {
                let proc_exit = matches!(
                    &error,
                    RuntimeError::HostCallFailed { module, name, .. }
                        if module == PROC_EXIT_MODULE && name == PROC_EXIT_NAME
                );
                let exit_code = self.exit_code.borrow_mut().take();
                if proc_exit {
                    if let Some(code) = exit_code {
                        return Ok(WasiInvocationOutcome::Exited(code));
                    }
                }
                Err(error)
            }
        }
    }
}
