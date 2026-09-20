//! Structured-control metadata and the validated bytecode boundary scanner.
//!
//! Keeping this scanner separate from execution makes opcode-width decoding and
//! control-boundary construction reviewable without navigating the interpreter
//! dispatch loop.

use wasm_parser::{decode_i32, decode_i64, decode_s33, Module, ValueType};

use super::{
    read_fixed_u32, read_fixed_u64, read_memarg, read_typed_select_type, read_u32_immediate,
    RuntimeError,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ControlKind {
    Function,
    Block,
    Loop,
    If,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BlockSignature {
    pub(super) params: Vec<ValueType>,
    pub(super) results: Vec<ValueType>,
}

#[derive(Debug, Clone)]
pub(super) struct ControlInfo {
    pub(super) kind: ControlKind,
    pub(super) body_pc: usize,
    pub(super) else_pc: Option<usize>,
    pub(super) end_pc: usize,
    pub(super) signature: BlockSignature,
}

#[derive(Debug, Clone)]
pub(super) struct ControlMap {
    pub(super) openers: Vec<Option<ControlInfo>>,
}

impl ControlMap {
    pub(super) fn info(&self, opener: usize) -> Result<ControlInfo, RuntimeError> {
        self.openers
            .get(opener)
            .and_then(Clone::clone)
            .ok_or(RuntimeError::ControlInvariant(
                "structured-control opener has no boundary metadata",
            ))
    }
}

#[derive(Debug, Clone)]
pub(super) struct PendingControl {
    pub(super) opener: usize,
    pub(super) kind: ControlKind,
    pub(super) body_pc: usize,
    pub(super) else_pc: Option<usize>,
    pub(super) signature: BlockSignature,
}

#[derive(Debug, Clone)]
pub(super) struct ExecControlFrame {
    pub(super) kind: ControlKind,
    pub(super) body_pc: usize,
    pub(super) end_pc: usize,
    pub(super) stack_height: usize,
    pub(super) param_types: Vec<ValueType>,
    pub(super) result_types: Vec<ValueType>,
}

impl ExecControlFrame {
    pub(super) fn label_types(&self) -> Vec<ValueType> {
        if self.kind == ControlKind::Loop {
            self.param_types.clone()
        } else {
            self.result_types.clone()
        }
    }
}

pub(super) fn build_control_map(module: &Module, code: &[u8]) -> Result<ControlMap, RuntimeError> {
    let mut openers = vec![None; code.len()];
    let mut pending = Vec::<PendingControl>::new();
    let mut pc = 0usize;

    while pc < code.len() {
        let offset = pc;
        let opcode = code[pc];
        pc += 1;
        match opcode {
            0x02..=0x04 => {
                let signature = read_block_signature(module, code, &mut pc)?;
                let kind = match opcode {
                    0x02 => ControlKind::Block,
                    0x03 => ControlKind::Loop,
                    0x04 => ControlKind::If,
                    _ => unreachable!(),
                };
                pending.push(PendingControl {
                    opener: offset,
                    kind,
                    body_pc: pc,
                    else_pc: None,
                    signature,
                });
            }
            0x05 => {
                let frame = pending.last_mut().ok_or(RuntimeError::ControlInvariant(
                    "else has no pending structured-control opener",
                ))?;
                if frame.kind != ControlKind::If || frame.else_pc.is_some() {
                    return Err(RuntimeError::ControlInvariant(
                        "else does not match exactly one if",
                    ));
                }
                frame.else_pc = Some(offset);
            }
            0x0b => {
                if let Some(frame) = pending.pop() {
                    openers[frame.opener] = Some(ControlInfo {
                        kind: frame.kind,
                        body_pc: frame.body_pc,
                        else_pc: frame.else_pc,
                        end_pc: offset,
                        signature: frame.signature,
                    });
                } else if pc != code.len() {
                    return Err(RuntimeError::ControlInvariant(
                        "function end occurs before final byte",
                    ));
                }
            }
            0x0c | 0x0d | 0x10 | 0x20..=0x26 | 0x3f | 0x40 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x0e => {
                let target_count = read_u32_immediate(code, &mut pc)?;
                for _ in 0..target_count {
                    let _ = read_u32_immediate(code, &mut pc)?;
                }
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x11 => {
                let _ = read_u32_immediate(code, &mut pc)?;
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0x1c => {
                let _ = read_typed_select_type(code, &mut pc)?;
            }
            0x28..=0x3e => {
                let _ = read_memarg(code, &mut pc)?;
            }
            0x41 => {
                let (_, used) = decode_i32(&code[pc..])?;
                pc += used;
            }
            0x42 => {
                let (_, used) = decode_i64(&code[pc..])?;
                pc += used;
            }
            0x43 => {
                let _ = read_fixed_u32(code, &mut pc)?;
            }
            0x44 => {
                let _ = read_fixed_u64(code, &mut pc)?;
            }
            0x00
            | 0x01
            | 0x0f
            | 0x1a
            | 0x1b
            | 0x45..=0x66
            | 0x67..=0x8a
            | 0x8b..=0xa6
            | 0xa7..=0xbf
            | 0xc0..=0xc4 => {}
            0xd0 => {
                let reference_type = *code.get(pc).ok_or(RuntimeError::UnsupportedOpcode(0xd0))?;
                pc += 1;
                if reference_type != 0x70 {
                    return Err(RuntimeError::UnsupportedOpcode(0xd0));
                }
            }
            0xd1 => {}
            0xd2 => {
                let _ = read_u32_immediate(code, &mut pc)?;
            }
            0xfd => {
                let subopcode = read_u32_immediate(code, &mut pc)?;
                match subopcode {
                    0..=11 => {
                        let _ = read_memarg(code, &mut pc)?;
                    }
                    84..=87 => {
                        let _ = read_memarg(code, &mut pc)?;
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated SIMD lane-load immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        let limit = match subopcode {
                            84 => 16,
                            85 => 8,
                            86 => 4,
                            87 => 2,
                            _ => unreachable!("matched SIMD lane-load opcode"),
                        };
                        if lane >= limit {
                            return Err(RuntimeError::ControlInvariant(
                                "validated SIMD lane-load lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    88..=91 => {
                        let _ = read_memarg(code, &mut pc)?;
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated SIMD lane-store immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        let limit = match subopcode {
                            88 => 16,
                            89 => 8,
                            90 => 4,
                            91 => 2,
                            _ => unreachable!("matched SIMD lane-store opcode"),
                        };
                        if lane >= limit {
                            return Err(RuntimeError::ControlInvariant(
                                "validated SIMD lane-store lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    12 => {
                        let end = pc.checked_add(16).ok_or(RuntimeError::ControlInvariant(
                            "validated v128.const immediate overflowed while scanning control",
                        ))?;
                        if code.get(pc..end).is_none() {
                            return Err(RuntimeError::ControlInvariant(
                                "validated v128.const immediate is missing while scanning control",
                            ));
                        }
                        pc = end;
                    }
                    13 => {
                        let end = pc.checked_add(16).ok_or(RuntimeError::ControlInvariant(
                            "validated i8x16.shuffle immediate overflowed while scanning control",
                        ))?;
                        let lanes = code.get(pc..end).ok_or(RuntimeError::ControlInvariant(
                            "validated i8x16.shuffle immediate is missing while scanning control",
                        ))?;
                        if lanes.iter().any(|lane| *lane >= 32) {
                            return Err(RuntimeError::ControlInvariant(
                                "validated i8x16.shuffle lane is out of bounds while scanning control",
                            ));
                        }
                        pc = end;
                    }
                    14
                    | 15
                    | 16
                    | 17
                    | 18
                    | 19
                    | 20
                    | 35..=54
                    | 55..=64
                    | 77..=83
                    | 94
                    | 95
                    | 96
                    | 97
                    | 98
                    | 103
                    | 104
                    | 105
                    | 106
                    | 99
                    | 100
                    | 101
                    | 102
                    | 107
                    | 108
                    | 109
                    | 111
                    | 112
                    | 114
                    | 115
                    | 116
                    | 117
                    | 118
                    | 119
                    | 120
                    | 121
                    | 122
                    | 123
                    | 124..=127
                    | 128
                    | 129
                    | 130
                    | 131
                    | 132
                    | 133
                    | 134
                    | 135..=138
                    | 139
                    | 140
                    | 141
                    | 163
                    | 164
                    | 167..=170
                    | 171..=173
                    | 203..=205
                    | 206
                    | 209
                    | 213
                    | 214..=219
                    | 220..=223
                    | 224
                    | 225
                    | 227
                    | 228..=235
                    | 236
                    | 237
                    | 239
                    | 240..=275
                    | 142
                    | 143
                    | 144
                    | 145
                    | 146
                    | 147
                    | 148
                    | 149
                    | 150
                    | 151
                    | 152
                    | 153
                    | 155
                    | 156..=159
                    | 174
                    | 177
                    | 181
                    | 188..=191 => {}
                    21..=23 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated i8x16 lane immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 16 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated i8x16 lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    24..=26 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated i16x8 lane immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 8 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated i16x8 lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    27 | 28 | 31 | 32 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated four-lane SIMD immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 4 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated four-lane SIMD lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    29 | 30 | 33 | 34 => {
                        let lane = *code.get(pc).ok_or(RuntimeError::ControlInvariant(
                            "validated two-lane SIMD immediate is missing while scanning control",
                        ))?;
                        pc += 1;
                        if lane >= 2 {
                            return Err(RuntimeError::ControlInvariant(
                                "validated two-lane SIMD lane is out of bounds while scanning control",
                            ));
                        }
                    }
                    92 | 93 => {
                        let _ = read_memarg(code, &mut pc)?;
                    }
                    _ => {
                        return Err(RuntimeError::UnsupportedPrefixedOpcode {
                            prefix: 0xfd,
                            subopcode,
                        });
                    }
                }
            }
            0xfc => {
                let subopcode = read_u32_immediate(code, &mut pc)?;
                match subopcode {
                    0..=7 => {}
                    8 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    9 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    10 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    11 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    12 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    13 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    14 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    15..=17 => {
                        let _ = read_u32_immediate(code, &mut pc)?;
                    }
                    _ => {
                        return Err(RuntimeError::UnsupportedPrefixedOpcode {
                            prefix: 0xfc,
                            subopcode,
                        })
                    }
                }
            }
            other => return Err(RuntimeError::UnsupportedOpcode(other)),
        }
    }

    if !pending.is_empty() {
        return Err(RuntimeError::ControlInvariant(
            "structured control is not fully closed",
        ));
    }
    Ok(ControlMap { openers })
}

pub(super) fn read_block_signature(
    module: &Module,
    code: &[u8],
    pc: &mut usize,
) -> Result<BlockSignature, RuntimeError> {
    let first = *code
        .get(*pc)
        .ok_or(RuntimeError::ControlInvariant("missing block type"))?;
    let immediate = match first {
        0x40 => {
            *pc += 1;
            return Ok(BlockSignature {
                params: Vec::new(),
                results: Vec::new(),
            });
        }
        0x7f => Some(ValueType::I32),
        0x7e => Some(ValueType::I64),
        0x7d => Some(ValueType::F32),
        0x7c => Some(ValueType::F64),
        _ => None,
    };
    if let Some(result) = immediate {
        *pc += 1;
        return Ok(BlockSignature {
            params: Vec::new(),
            results: vec![result],
        });
    }

    let (raw, used) = decode_s33(&code[*pc..])?;
    *pc += used;
    if raw < 0 {
        return Err(RuntimeError::UnsupportedBlockType(first));
    }
    let type_index = u32::try_from(raw)
        .map_err(|_| RuntimeError::ControlInvariant("block type index exceeds u32"))?;
    let ty = module
        .types
        .get(type_index as usize)
        .ok_or(RuntimeError::BlockTypeIndexOutOfBounds(type_index))?;
    Ok(BlockSignature {
        params: ty.params.clone(),
        results: ty.results.clone(),
    })
}
