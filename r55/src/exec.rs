use alloy_core::primitives::Keccak256;
use core::{cell::RefCell, ops::Range};
use eth_riscv_interpreter::setup_from_elf;
use eth_riscv_syscalls::Syscall;
use revm::{
    handler::register::EvmHandler,
    interpreter::{
        CallInputs, CallScheme, CallValue, Host, InstructionResult, Interpreter, InterpreterAction,
        InterpreterResult, SharedMemory,
    },
    primitives::{address, Address, Bytes, ExecutionResult, Log, Output, TransactTo, B256, U256},
    Database, Evm, Frame, FrameOrResult, InMemoryDB,
};
use rvemu::{emulator::Emulator, exception::Exception};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

use super::error::{Error, Result, TxResult};

pub fn deploy_contract(db: &mut InMemoryDB, bytecode: Bytes) -> Result<Address> {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("0000000000000000000000000000000000000001");
            tx.transact_to = TransactTo::Create;
            tx.data = bytecode;
            tx.value = U256::from(0);
        })
        .append_handler_register(handle_register)
        .build();
    evm.cfg_mut().limit_contract_code_size = Some(usize::MAX);

    let result = evm.transact_commit()?;

    match result {
        ExecutionResult::Success {
            output: Output::Create(_value, Some(addr)),
            ..
        } => {
            println!("Deployed at addr: {:?}", addr);
            Ok(addr)
        }
        result => Err(Error::UnexpectedExecResult(result)),
    }
}

pub fn run_tx(db: &mut InMemoryDB, addr: &Address, calldata: Vec<u8>) -> Result<TxResult> {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("0000000000000000000000000000000000000001");
            tx.transact_to = TransactTo::Call(*addr);
            tx.data = calldata.into();
            tx.value = U256::from(0);
            tx.gas_price = U256::from(42);
            tx.gas_limit = 100_000;
        })
        .append_handler_register(handle_register)
        .build();

    let result = evm.transact_commit()?;

    match result {
        ExecutionResult::Success {
            reason,
            gas_used,
            gas_refunded: _,
            logs,
            output: Output::Call(value),
            ..
        } => {
            println!("Tx result: {:?}", value);
            Ok(TxResult {
                output: value.into(),
                logs,
                gas_used,
                status: true,
            })
        }
        result => Err(Error::UnexpectedExecResult(result)),
    }
}

#[derive(Debug)]
struct RVEmu {
    emu: Emulator,
    returned_data_destiny: Option<Range<u64>>,
}

fn riscv_context(frame: &Frame) -> Option<RVEmu> {
    let interpreter = frame.interpreter();

    let Some((0xFF, bytecode)) = interpreter.bytecode.split_first() else {
        return None;
    };

    match setup_from_elf(bytecode, &interpreter.contract.input) {
        Ok(emu) => Some(RVEmu {
            emu,
            returned_data_destiny: None,
        }),
        Err(err) => {
            println!("Failed to setup from ELF: {err}");
            None
        }
    }
}

pub fn handle_register<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
    let call_stack = Rc::<RefCell<Vec<_>>>::new(RefCell::new(Vec::new()));

    // create a riscv context on call frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.call.clone();
    handler.execution.call = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(riscv_context(frame));
        }
        result
    });

    // create a riscv context on create frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.create.clone();
    handler.execution.create = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(riscv_context(frame));
        }
        result
    });

    // execute riscv context or old logic.
    let old_handle = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, instraction_table, ctx| {
        let result = if let Some(Some(riscv_context)) = call_stack.borrow_mut().first_mut() {
            execute_riscv(riscv_context, frame.interpreter_mut(), memory, ctx)?
        } else {
            old_handle(frame, memory, instraction_table, ctx)?
        };

        // if it is return pop the stack.
        if result.is_return() {
            call_stack.borrow_mut().pop();
        }
        Ok(result)
    });
}

fn execute_riscv(
    rvemu: &mut RVEmu,
    interpreter: &mut Interpreter,
    shared_memory: &mut SharedMemory,
    host: &mut dyn Host,
) -> Result<InterpreterAction> {
    let emu = &mut rvemu.emu;
    emu.cpu.is_count = true;
    let returned_data_destiny = &mut rvemu.returned_data_destiny;
    if let Some(destiny) = std::mem::take(returned_data_destiny) {
        let data = emu.cpu.bus.get_dram_slice(destiny)?;
        data.copy_from_slice(shared_memory.slice(0, data.len()))
    }

    let return_revert = |interpreter: &mut Interpreter, gas_used: u64| {
        let _ = interpreter.gas.record_cost(gas_used);
        Ok(InterpreterAction::Return {
            result: InterpreterResult {
                result: InstructionResult::Revert,
                // return empty bytecode
                output: Bytes::new(),
                gas: interpreter.gas,
            },
        })
    };
    // Tracks gas usage across EVM host calls
    let mut evm_gas = 0;
    // Run emulator and capture ecalls
    loop {
        let run_result = emu.start();
        match run_result {
            Err(Exception::EnvironmentCallFromMMode) => {
                let t0: u64 = emu.cpu.xregs.read(5);

                let Ok(syscall) = Syscall::try_from(t0 as u8) else {
                    println!("Unhandled syscall: {:?}", t0);
                    return return_revert(interpreter, evm_gas);
                };

                match syscall {
                    Syscall::Return => {
                        let ret_offset: u64 = emu.cpu.xregs.read(10);
                        let ret_size: u64 = emu.cpu.xregs.read(11);

                        let r55_gas = r55_gas_used(&emu.cpu.inst_counter);
                        let data_bytes = dram_slice(emu, ret_offset, ret_size)?;

                        let total_cost = r55_gas + evm_gas;
                        println!(
                            "evm gas: {}, r55 gas: {}, total cost: {}",
                            evm_gas, r55_gas, total_cost
                        );
                        let in_limit = interpreter.gas.record_cost(total_cost);
                        if !in_limit {
                            return Ok(InterpreterAction::Return {
                                result: InterpreterResult {
                                    result: InstructionResult::OutOfGas,
                                    output: Bytes::new(),
                                    gas: interpreter.gas,
                                },
                            });
                        }

                        return Ok(InterpreterAction::Return {
                            result: InterpreterResult {
                                result: InstructionResult::Return,
                                output: data_bytes.to_vec().into(),
                                gas: interpreter.gas, // FIXME: gas is not correct
                            },
                        });
                    }
                    Syscall::SLoad => {
                        let key: u64 = emu.cpu.xregs.read(10);
                        match host.sload(interpreter.contract.target_address, U256::from(key)) {
                            Some((value, is_cold)) => {
                                let limbs = value.as_limbs();
                                emu.cpu.xregs.write(10, limbs[0]);
                                emu.cpu.xregs.write(11, limbs[1]);
                                emu.cpu.xregs.write(12, limbs[2]);
                                emu.cpu.xregs.write(13, limbs[3]);
                                if is_cold {
                                    evm_gas += 2100
                                } else {
                                    evm_gas += 100
                                }
                            }
                            _ => {
                                return return_revert(interpreter, evm_gas);
                            }
                        }
                    }
                    Syscall::SStore => {
                        let key: u64 = emu.cpu.xregs.read(10);
                        let first: u64 = emu.cpu.xregs.read(11);
                        let second: u64 = emu.cpu.xregs.read(12);
                        let third: u64 = emu.cpu.xregs.read(13);
                        let fourth: u64 = emu.cpu.xregs.read(14);
                        let result = host.sstore(
                            interpreter.contract.target_address,
                            U256::from(key),
                            U256::from_limbs([first, second, third, fourth]),
                        );
                        if let Some(result) = result {
                            if result.is_cold {
                                evm_gas += 2200
                            } else {
                                evm_gas += 100
                            }
                        }
                    }
                    Syscall::Call => {
                        let a0: u64 = emu.cpu.xregs.read(10);
                        let address =
                            Address::from_slice(emu.cpu.bus.get_dram_slice(a0..(a0 + 20))?);
                        let value: u64 = emu.cpu.xregs.read(11);
                        let args_offset: u64 = emu.cpu.xregs.read(12);
                        let args_size: u64 = emu.cpu.xregs.read(13);
                        let ret_offset = emu.cpu.xregs.read(14);
                        let ret_size = emu.cpu.xregs.read(15);

                        *returned_data_destiny = Some(ret_offset..(ret_offset + ret_size));

                        let tx = &host.env().tx;
                        return Ok(InterpreterAction::Call {
                            inputs: Box::new(CallInputs {
                                input: emu
                                    .cpu
                                    .bus
                                    .get_dram_slice(args_offset..(args_offset + args_size))?
                                    .to_vec()
                                    .into(),
                                gas_limit: tx.gas_limit,
                                target_address: address,
                                bytecode_address: address,
                                caller: interpreter.contract.target_address,
                                value: CallValue::Transfer(U256::from_le_bytes(
                                    value.to_le_bytes(),
                                )),
                                scheme: CallScheme::Call,
                                is_static: false,
                                is_eof: false,
                                return_memory_offset: 0..ret_size as usize,
                            }),
                        });
                    }
                    Syscall::Revert => {
                        return Ok(InterpreterAction::Return {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output: Bytes::from(0u32.to_le_bytes()), //TODO: return revert(0,0)
                                gas: interpreter.gas, // FIXME: gas is not correct
                            },
                        });
                    }
                    Syscall::Caller => {
                        let caller = interpreter.contract.caller;
                        // Break address into 3 u64s and write to registers
                        let caller_bytes = caller.as_slice();
                        let first_u64 = u64::from_be_bytes(caller_bytes[0..8].try_into()?);
                        emu.cpu.xregs.write(10, first_u64);
                        let second_u64 = u64::from_be_bytes(caller_bytes[8..16].try_into()?);
                        emu.cpu.xregs.write(11, second_u64);
                        let mut padded_bytes = [0u8; 8];
                        padded_bytes[..4].copy_from_slice(&caller_bytes[16..20]);
                        let third_u64 = u64::from_be_bytes(padded_bytes);
                        emu.cpu.xregs.write(12, third_u64);
                    }
                    Syscall::Keccak256 => {
                        let ret_offset: u64 = emu.cpu.xregs.read(10);
                        let ret_size: u64 = emu.cpu.xregs.read(11);
                        let data_bytes = dram_slice(emu, ret_offset, ret_size)?;

                        let mut hasher = Keccak256::new();
                        hasher.update(data_bytes);
                        let hash: [u8; 32] = hasher.finalize().into();

                        // Write the hash to the emulator's registers
                        emu.cpu
                            .xregs
                            .write(10, u64::from_le_bytes(hash[0..8].try_into()?));
                        emu.cpu
                            .xregs
                            .write(11, u64::from_le_bytes(hash[8..16].try_into()?));
                        emu.cpu
                            .xregs
                            .write(12, u64::from_le_bytes(hash[16..24].try_into()?));
                        emu.cpu
                            .xregs
                            .write(13, u64::from_le_bytes(hash[24..32].try_into()?));
                    }
                    Syscall::CallValue => {
                        let value = interpreter.contract.call_value;
                        let limbs = value.into_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::BaseFee => {
                        let value = host.env().block.basefee;
                        let limbs = value.as_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::ChainId => {
                        let value = host.env().cfg.chain_id;
                        emu.cpu.xregs.write(10, value);
                    }
                    Syscall::GasLimit => {
                        let limit = host.env().block.gas_limit;
                        let limbs = limit.as_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::Number => {
                        let number = host.env().block.number;
                        let limbs = number.as_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::Timestamp => {
                        let timestamp = host.env().block.timestamp;
                        let limbs = timestamp.as_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::GasPrice => {
                        let value = host.env().tx.gas_price;
                        let limbs = value.as_limbs();
                        emu.cpu.xregs.write(10, limbs[0]);
                        emu.cpu.xregs.write(11, limbs[1]);
                        emu.cpu.xregs.write(12, limbs[2]);
                        emu.cpu.xregs.write(13, limbs[3]);
                    }
                    Syscall::Origin => {
                        // Syscall::Origin
                        let origin = host.env().tx.caller;
                        // Break address into 3 u64s and write to registers
                        let origin_bytes = origin.as_slice();

                        let first_u64 = u64::from_be_bytes(origin_bytes[0..8].try_into().unwrap());
                        emu.cpu.xregs.write(10, first_u64);

                        let second_u64 =
                            u64::from_be_bytes(origin_bytes[8..16].try_into().unwrap());
                        emu.cpu.xregs.write(11, second_u64);

                        let mut padded_bytes = [0u8; 8];
                        padded_bytes[..4].copy_from_slice(&origin_bytes[16..20]);
                        let third_u64 = u64::from_be_bytes(padded_bytes);
                        emu.cpu.xregs.write(12, third_u64);
                    }
                    Syscall::Log => {
                        let data_ptr: u64 = emu.cpu.xregs.read(10);
                        let data_size: u64 = emu.cpu.xregs.read(11);
                        let topics_ptr: u64 = emu.cpu.xregs.read(12);
                        let topics_size: u64 = emu.cpu.xregs.read(13);

                        // Read data
                        let data_slice =
                            match emu.cpu.bus.get_dram_slice(data_ptr..(data_ptr + data_size)) {
                                Ok(slice) => slice,
                                Err(_) => &mut [],
                            };
                        let data = data_slice.to_vec();

                        // Read topics
                        let topics_start = topics_ptr;
                        let topics_end = topics_ptr + topics_size * 32;
                        let topics_slice =
                            match emu.cpu.bus.get_dram_slice(topics_start..topics_end) {
                                Ok(slice) => slice,
                                Err(_) => &mut [],
                            };
                        let topics = topics_slice
                            .chunks(32)
                            .map(|chunk| B256::from_slice(chunk))
                            .collect::<Vec<B256>>();

                        host.log(Log::new_unchecked(
                            interpreter.contract.target_address,
                            topics,
                            data.into(),
                        ));
                    }
                }
            }
            _ => {
                let total_cost = r55_gas_used(&emu.cpu.inst_counter) + evm_gas;
                return return_revert(interpreter, total_cost);
            }
        }
    }
}

/// Returns RISC-V DRAM slice in a given size range, starts with a given offset
fn dram_slice(emu: &mut Emulator, ret_offset: u64, ret_size: u64) -> Result<&mut [u8]> {
    if ret_size != 0 {
        Ok(emu
            .cpu
            .bus
            .get_dram_slice(ret_offset..(ret_offset + ret_size))?)
    } else {
        Ok(&mut [])
    }
}

fn r55_gas_used(inst_count: &BTreeMap<String, u64>) -> u64 {
    let total_cost = inst_count
        .iter()
        .map(|(inst_name, count)|
            // Gas cost = number of instructions * cycles per instruction
            match inst_name.as_str() {
                // Gas map to approximate cost of each instruction
                // References:
                // http://ithare.com/infographics-operation-costs-in-cpu-clock-cycles/
                // https://www.evm.codes/?fork=cancun#54
                // Division and remainder
                s if s.starts_with("div") || s.starts_with("rem") => count * 25,
                // Multiplications
                s if s.starts_with("mul") => count * 5,
                // Loads
                "lb" | "lh" | "lw" | "ld" | "lbu" | "lhu" | "lwu" => count * 3, // Cost analagous to `MLOAD`
                // Stores
                "sb" | "sh" | "sw" | "sd" | "sc.w" | "sc.d" => count * 3, // Cost analagous to `MSTORE`
                // Branching
                "beq" | "bne" | "blt" | "bge" | "bltu" | "bgeu" | "jal" | "jalr" => count * 3,
                _ => *count, // All other instructions including `add` and `sub`
        })
        .sum::<u64>();

    // This is the minimum 'gas used' to ABI decode 'empty' calldata into Rust type arguments. Real calldata will take more gas.
    // Internalising this would focus gas metering more on the function logic
    let abi_decode_cost = 9_175_538;

    total_cost - abi_decode_cost
}
