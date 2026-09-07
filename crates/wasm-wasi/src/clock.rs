use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::{ERRNO_FAULT, ERRNO_INVAL, ERRNO_SUCCESS};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const CLOCK_RES_GET_NAME: &str = "clock_res_get";
const CLOCK_TIME_GET_NAME: &str = "clock_time_get";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum WasiClockId {
    Realtime = 0,
    Monotonic = 1,
    ProcessCpuTime = 2,
    ThreadCpuTime = 3,
}

impl WasiClockId {
    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClockSnapshot {
    resolution_ns: u64,
    time_ns: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ClockSet {
    snapshots: [Option<ClockSnapshot>; 4],
}

impl ClockSet {
    pub(crate) fn configure(&mut self, id: WasiClockId, resolution_ns: u64, time_ns: u64) {
        self.snapshots[id.index()] = Some(ClockSnapshot {
            resolution_ns,
            time_ns,
        });
    }

    fn snapshot(&self, raw_id: i32) -> Option<ClockSnapshot> {
        let raw_id = raw_id as u32;
        if raw_id > WasiClockId::ThreadCpuTime as u32 {
            return None;
        }
        self.snapshots[raw_id as usize]
    }

    pub(crate) fn register(&self, registry: &mut HostRegistry) -> Result<(), HostRegistryError> {
        let resolution_clocks = *self;
        registry.register_values(
            WASI_MODULE,
            CLOCK_RES_GET_NAME,
            vec![ValueType::I32, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(clock_id), Value::I32(result)] = args else {
                    return Err(HostError::message(
                        "validated wasi clock_res_get signature received invalid arguments",
                    ));
                };

                let Some(snapshot) = resolution_clocks.snapshot(*clock_id) else {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                };
                let result = *result as u32;
                if context.read_memory(result, 8).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context
                    .write_memory(result, &snapshot.resolution_ns.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )?;

        let time_clocks = *self;
        registry.register_values(
            WASI_MODULE,
            CLOCK_TIME_GET_NAME,
            vec![ValueType::I32, ValueType::I64, ValueType::I32],
            vec![ValueType::I32],
            HostCapabilities::MEMORY_READ_WRITE,
            move |context, args| {
                let [Value::I32(clock_id), Value::I64(_precision), Value::I32(result)] = args
                else {
                    return Err(HostError::message(
                        "validated wasi clock_time_get signature received invalid arguments",
                    ));
                };

                let Some(snapshot) = time_clocks.snapshot(*clock_id) else {
                    return Ok(vec![Value::I32(ERRNO_INVAL)]);
                };
                let result = *result as u32;
                if context.read_memory(result, 8).is_err() {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }
                if context
                    .write_memory(result, &snapshot.time_ns.to_le_bytes())
                    .is_err()
                {
                    return Ok(vec![Value::I32(ERRNO_FAULT)]);
                }

                Ok(vec![Value::I32(ERRNO_SUCCESS)])
            },
        )
    }
}
