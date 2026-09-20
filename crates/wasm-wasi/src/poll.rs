use wasm_parser::ValueType;
use wasm_runtime::{HostCapabilities, HostError, HostRegistry, HostRegistryError, Value};

use crate::clock::ClockSet;
use crate::{ERRNO_FAULT, ERRNO_INVAL, ERRNO_NOTSUP, ERRNO_SUCCESS};

const WASI_MODULE: &str = "wasi_snapshot_preview1";
const POLL_ONEOFF_NAME: &str = "poll_oneoff";

const SUBSCRIPTION_SIZE: usize = 48;
const EVENT_SIZE: usize = 32;
const MAX_SUBSCRIPTIONS: usize = 64;

const EVENTTYPE_CLOCK: u8 = 0;
const EVENTTYPE_FD_READ: u8 = 1;
const EVENTTYPE_FD_WRITE: u8 = 2;
const SUBCLOCKFLAGS_ABSTIME: u16 = 1;

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("u16 field width"))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32 field width"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64 field width"))
}

fn ready_clock_event(
    clocks: ClockSet,
    subscription: &[u8],
) -> Result<[u8; EVENT_SIZE], i32> {
    let userdata = read_u64(subscription, 0);
    let event_type = subscription[8];
    match event_type {
        EVENTTYPE_CLOCK => {}
        EVENTTYPE_FD_READ | EVENTTYPE_FD_WRITE => return Err(ERRNO_NOTSUP),
        _ => return Err(ERRNO_INVAL),
    }

    let clock_id = read_u32(subscription, 16);
    if clock_id > 1 {
        return Err(ERRNO_INVAL);
    }
    let timeout = read_u64(subscription, 24);
    let _precision = read_u64(subscription, 32);
    let flags = read_u16(subscription, 40);
    if flags & !SUBCLOCKFLAGS_ABSTIME != 0 {
        return Err(ERRNO_INVAL);
    }

    let Some(now) = clocks.time_ns(clock_id as i32) else {
        return Err(ERRNO_INVAL);
    };
    let ready = if flags & SUBCLOCKFLAGS_ABSTIME != 0 {
        timeout <= now
    } else {
        timeout == 0
    };
    if !ready {
        return Err(ERRNO_NOTSUP);
    }

    let mut event = [0u8; EVENT_SIZE];
    event[0..8].copy_from_slice(&userdata.to_le_bytes());
    event[8..10].copy_from_slice(&(ERRNO_SUCCESS as u16).to_le_bytes());
    event[10] = EVENTTYPE_CLOCK;
    Ok(event)
}

pub(crate) fn register(
    registry: &mut HostRegistry,
    clocks: ClockSet,
) -> Result<(), HostRegistryError> {
    registry.register_values(
        WASI_MODULE,
        POLL_ONEOFF_NAME,
        vec![
            ValueType::I32,
            ValueType::I32,
            ValueType::I32,
            ValueType::I32,
        ],
        vec![ValueType::I32],
        HostCapabilities::MEMORY_READ_WRITE,
        move |context, args| {
            let [
                Value::I32(input),
                Value::I32(output),
                Value::I32(nsubscriptions),
                Value::I32(nevents),
            ] = args
            else {
                return Err(HostError::message(
                    "validated wasi poll_oneoff signature received invalid arguments",
                ));
            };

            let count = *nsubscriptions as u32 as usize;
            if count == 0 || count > MAX_SUBSCRIPTIONS {
                return Ok(vec![Value::I32(ERRNO_INVAL)]);
            }

            let input_len = count
                .checked_mul(SUBSCRIPTION_SIZE)
                .ok_or_else(|| HostError::message("poll_oneoff input length overflow"))?;
            let output_len = count
                .checked_mul(EVENT_SIZE)
                .ok_or_else(|| HostError::message("poll_oneoff output length overflow"))?;

            let input_address = *input as u32;
            let output_address = *output as u32;
            let nevents_address = *nevents as u32;

            let subscriptions = match context.read_memory(input_address, input_len) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(vec![Value::I32(ERRNO_FAULT)]),
            };
            if context.read_memory(output_address, output_len).is_err()
                || context.read_memory(nevents_address, 4).is_err()
            {
                return Ok(vec![Value::I32(ERRNO_FAULT)]);
            }

            let mut events = Vec::with_capacity(output_len);
            for subscription in subscriptions.chunks_exact(SUBSCRIPTION_SIZE) {
                let event = match ready_clock_event(clocks, subscription) {
                    Ok(event) => event,
                    Err(errno) => return Ok(vec![Value::I32(errno)]),
                };
                events.extend_from_slice(&event);
            }

            if context.write_memory(output_address, &events).is_err()
                || context
                    .write_memory(nevents_address, &(count as u32).to_le_bytes())
                    .is_err()
            {
                return Ok(vec![Value::I32(ERRNO_FAULT)]);
            }

            Ok(vec![Value::I32(ERRNO_SUCCESS)])
        },
    )
}
