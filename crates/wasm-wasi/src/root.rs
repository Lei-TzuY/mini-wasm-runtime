//! Bounded WASI Preview1 host capabilities for `mini-wasm-runtime`.
//!
//! Existing descriptor, argument, and environment capabilities remain isolated in the original
//! implementation module. This crate root layers typed process termination over that stable API.

#[path = "lib.rs"]
mod base;

pub use base::{
    OutputBuffer, ERRNO_BADF, ERRNO_FAULT, ERRNO_INVAL, ERRNO_SUCCESS, FILETYPE_CHARACTER_DEVICE,
    RIGHTS_FD_READ, RIGHTS_FD_WRITE,
};

use std::{cell::RefCell, rc::Rc};
use wasm_parser::ValueType;
use wasm_runtime::{
    HostCapabilities, HostError, HostRegistry, HostRegistryError, Instance, RuntimeError, Value,
};

const PROC_EXIT_MODULE: &str = "wasi_snapshot_preview1";
const PROC_EXIT_NAME: &str = "proc_exit";
const PROC_EXIT_CONTROL_TRANSFER: &str = "wasi proc_exit control transfer";

#[derive(Debug, Clone, PartialEq)]
pub enum WasiInvocationOutcome {
    Returned(Vec<Value>),
    Exited(u32),
}

#[derive(Debug, Clone)]
pub struct WasiPreview1 {
    base: base::WasiPreview1,
    exit_code: Rc<RefCell<Option<u32>>>,
}

impl Default for WasiPreview1 {
    fn default() -> Self {
        Self {
            base: base::WasiPreview1::new(),
            exit_code: Rc::new(RefCell::new(None)),
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

    pub fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        self.base.register(registry)?;

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
