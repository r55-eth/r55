use alloy_core::primitives::{Keccak256, U32};
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
use tracing::{debug, info, trace, warn};

use super::error::{Error, Result, TxResult};
use super::gas;
use super::syscall_gas;

const R5_REST_OF_RAM_INIT: u64 = 0x80300000; // Defined at `r5-rust-rt.x`

pub fn deploy_contract(
    db: &mut InMemoryDB,
    bytecode: Bytes,
    encoded_args: Option<Vec<u8>>,
) -> Result<Address> {
    // Craft initcode: [0xFF][codesize][bytecode][constructor_args]
    let codesize = U32::from(bytecode.len());
    debug!("CODESIZE: {}", codesize);

    let mut init_code = Vec::new();
    init_code.push(0xff);
    init_code.extend_from_slice(&Bytes::from(codesize.to_be_bytes_vec()));
    init_code.extend_from_slice(&bytecode);
    if let Some(args) = encoded_args {
        debug!("ENCODED_ARGS: {:#?}", Bytes::from(args.clone()));
        init_code.extend_from_slice(&args);
    }
    debug!("INITCODE SIZE: {}", init_code.len());

    // Run CREATE tx
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("000000000000000000000000000000000000000A");
            tx.transact_to = TransactTo::Create;
            // tx.data = bytecode;
            tx.data = Bytes::from(init_code);
            tx.value = U256::from(0);
        })
        .append_handler_register(handle_register)
        .build();
    evm.cfg_mut().limit_contract_code_size = Some(usize::MAX);

    let result = evm.transact_commit()?;

    match result {
        ExecutionResult::Success {
            output: Output::Create(_value, Some(addr)),
            logs,
            ..
        } => {
            info!(
                "NEW DEPLOYMENT:\n> contract address: {:?}\n> logs: {:#?}",
                addr, logs
            );
            Ok(addr)
        }
        result => Err(Error::UnexpectedExecResult(result)),
    }
}

pub fn run_tx(db: &mut InMemoryDB, addr: &Address, calldata: Vec<u8>) -> Result<TxResult> {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("000000000000000000000000000000000000000A");
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
            reason: _,
            gas_used,
            gas_refunded: _,
            logs,
            output: Output::Call(value),
            ..
        } => {
            debug!("Tx result: {:?}", value);
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
        warn!("NOT RISCV CONTRACT!");
        return None;
    };

    let (code, calldata) = if frame.is_create() {
        let (code_size, init_code) = bytecode.split_at(4);
        let Some((0xFF, bytecode)) = init_code.split_first() else {
            warn!("NOT RISCV CONTRACT!");
            return None;
        };
        let code_size = U32::from_be_slice(code_size).to::<usize>() - 1; // deduct control byte `0xFF`
        let end_of_args = init_code.len() - 34; // deduct control byte + revm appends 1 empty (32 byte) word

        (&bytecode[..code_size], &bytecode[code_size..end_of_args])
    } else if frame.is_call() {
        (bytecode, interpreter.contract.input.as_ref())
    } else {
        todo!("Support EOF")
    };

    match setup_from_elf(code, calldata) {
        Ok(emu) => Some(RVEmu {
            emu,
            returned_data_destiny: None,
        }),
        Err(err) => {
            warn!("Failed to setup from ELF: {err}");
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
            trace!("Creating new CALL frame");
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
            trace!("Creating new CREATE frame");
            call_stack_inner.borrow_mut().push(riscv_context(frame));
        }
        result
    });

    // execute riscv context or old logic.
    let old_handle = handler.execution.execute_frame.clone();
    handler.execution.execute_frame = Arc::new(move |frame, memory, instraction_table, ctx| {
        let depth = call_stack.borrow().len() - 1;

        // use last frame as stack is LIFO
        let mut result = if let Some(Some(riscv_context)) = call_stack.borrow_mut().last_mut() {
            debug!(
                "=== [FRAME-{}] Contract: {} ============-",
                depth,
                frame.interpreter().contract.target_address,
            );
            execute_riscv(riscv_context, frame.interpreter_mut(), memory, ctx)?
        } else {
            debug!("=== [OLD Handler] ==================--");
            old_handle(frame, memory, instraction_table, ctx)?
        };

        // if action is return, pop the stack.
        if result.is_return() {
            call_stack.borrow_mut().pop();

            let is_last = call_stack.borrow().is_empty();
            debug!(
                "=== [FRAME-{}] Ouput: RETURN {}",
                depth,
                if is_last { "(last)" } else { "" }
            );

            if !is_last {
                if let Some(Some(parent)) = call_stack.borrow_mut().last_mut() {
                    if let Some(return_range) = &parent.returned_data_destiny {
                        if let InterpreterAction::Return { result: res } = &mut result {
                            // get allocated memory slice
                            let return_memory = parent
                                .emu
                                .cpu
                                .bus
                                .get_dram_slice(return_range.clone())
                                .expect("unable to get memory from return range");

                            debug!("- Return data: {:?}", res.output);
                            debug!("- Memory range: {:?}", return_range);
                            debug!("- Memory size: {}", return_memory.len());

                            // write return data to parent's memory
                            if res.output.len() == return_memory.len() {
                                return_memory.copy_from_slice(&res.output);
                            } else {
                                warn!("Unexpected return data size!");
                            }
                        }
                    }
                }
            }
        }
        debug!("=== [Frame-{}] {:#?}", depth, frame.interpreter().gas);

        Ok(result)
    });
}

fn execute_riscv(
    rvemu: &mut RVEmu,
    interpreter: &mut Interpreter,
    _shared_memory: &mut SharedMemory,
    host: &mut dyn Host,
) -> Result<InterpreterAction> {
    debug!(
        "{} RISC-V execution:  PC: {:#x}  Return data dst: {:#?}",
        if rvemu.emu.cpu.pc == R5_REST_OF_RAM_INIT {
            "Starting"
        } else {
            "Resuming"
        },
        rvemu.emu.cpu.pc,
        &rvemu.returned_data_destiny
    );

    let emu = &mut rvemu.emu;
    emu.cpu.is_count = true;

    let returned_data_destiny = &mut rvemu.returned_data_destiny;
    if let Some(destiny) = std::mem::take(returned_data_destiny) {
        let data = emu.cpu.bus.get_dram_slice(destiny)?;
        debug!("Loaded return data: {}", Bytes::copy_from_slice(data));
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

    // Run emulator and capture ecalls
    loop {
        let run_result = emu.start();
        match run_result {
            Err(Exception::EnvironmentCallFromMMode) => {
                let t0: u64 = emu.cpu.xregs.read(5);

                let Ok(syscall) = Syscall::try_from(t0 as u8) else {
                    warn!("Unhandled syscall: {:?}", t0);
                    return return_revert(interpreter, interpreter.gas.spent());
                };
                debug!("[Syscall::{} - {:#04x}]", syscall, t0);

                match syscall {
                    Syscall::Return => {
                        let ret_offset: u64 = emu.cpu.xregs.read(10);
                        let ret_size: u64 = emu.cpu.xregs.read(11);

                        let r55_gas = r55_gas_used(&emu.cpu.inst_counter);
                        debug!("> Total R55 gas: {}", r55_gas);

                        // RETURN logs the gas of the whole risc-v instruction set
                        syscall_gas!(interpreter, r55_gas);

                        let data_bytes = dram_slice(emu, ret_offset, ret_size)?;

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
                        debug!(
                            "> SLOAD ({}) - Key: {}",
                            interpreter.contract.target_address, key
                        );
                        match host.sload(interpreter.contract.target_address, U256::from(key)) {
                            Some((value, is_cold)) => {
                                debug!(
                                    "> SLOAD ({}) - Value: {}",
                                    interpreter.contract.target_address, value
                                );
                                let limbs = value.as_limbs();
                                emu.cpu.xregs.write(10, limbs[0]);
                                emu.cpu.xregs.write(11, limbs[1]);
                                emu.cpu.xregs.write(12, limbs[2]);
                                emu.cpu.xregs.write(13, limbs[3]);
                                syscall_gas!(
                                    interpreter,
                                    if is_cold {
                                        gas::SLOAD_COLD
                                    } else {
                                        gas::SLOAD_WARM
                                    }
                                );
                            }
                            _ => {
                                return return_revert(interpreter, interpreter.gas.spent());
                            }
                        }
                    }
                    Syscall::SStore => {
                        let key: u64 = emu.cpu.xregs.read(10);
                        let first: u64 = emu.cpu.xregs.read(11);
                        let second: u64 = emu.cpu.xregs.read(12);
                        let third: u64 = emu.cpu.xregs.read(13);
                        let fourth: u64 = emu.cpu.xregs.read(14);
                        debug!(
                            "> SSTORE ({}) - Key: {}, Value: {}",
                            interpreter.contract.target_address,
                            key,
                            U256::from_limbs([first, second, third, fourth])
                        );
                        let result = host.sstore(
                            interpreter.contract.target_address,
                            U256::from(key),
                            U256::from_limbs([first, second, third, fourth]),
                        );
                        if let Some(result) = result {
                            syscall_gas!(
                                interpreter,
                                if result.is_cold {
                                    gas::SSTORE_COLD
                                } else {
                                    gas::SSTORE_WARM
                                }
                            );
                        }
                    }
                    Syscall::Call => {
                        let a0: u64 = emu.cpu.xregs.read(10);
                        let a1: u64 = emu.cpu.xregs.read(11);
                        let a2: u64 = emu.cpu.xregs.read(12);
                        let addr = Address::from_word(U256::from_limbs([a0, a1, a2, 0]).into());
                        let value: u64 = emu.cpu.xregs.read(13);

                        // Get calldata
                        let args_offset: u64 = emu.cpu.xregs.read(14);
                        let args_size: u64 = emu.cpu.xregs.read(15);
                        let calldata: Bytes = emu
                            .cpu
                            .bus
                            .get_dram_slice(args_offset..(args_offset + args_size))
                            .unwrap_or(&mut [])
                            .to_vec()
                            .into();

                        // Store where return data should go
                        let ret_offset = emu.cpu.xregs.read(16);
                        let ret_size = emu.cpu.xregs.read(17);
                        debug!(
                            "> Return data will be written to: {}..{}",
                            ret_offset,
                            ret_offset + ret_size
                        );

                        // Initialize memory region for return data
                        let return_memory = emu
                            .cpu
                            .bus
                            .get_dram_slice(ret_offset..(ret_offset + ret_size))?;
                        return_memory.fill(0);
                        rvemu.returned_data_destiny = Some(ret_offset..(ret_offset + ret_size));

                        // Calculate gas cost of the call
                        // TODO: check correctness (tried using evm.codes as ref but i'm no gas wizard)
                        // TODO: unsure whether memory expansion cost is missing (should be captured in the risc-v costs)
                        let (empty_account_cost, addr_access_cost) = match host.load_account(addr) {
                            Some(account) => {
                                if account.is_cold {
                                    (0, gas::CALL_NEW_ACCOUNT)
                                } else {
                                    (0, gas::CALL_BASE)
                                }
                            }
                            None => (gas::CALL_EMPTY_ACCOUNT, gas::CALL_NEW_ACCOUNT),
                        };
                        let value_cost = if value != 0 { gas::CALL_VALUE } else { 0 };
                        let call_gas_cost = empty_account_cost + addr_access_cost + value_cost;
                        syscall_gas!(interpreter, call_gas_cost);

                        // proactively spend gas limit as the remaining will be refunded (otherwise it underflows)
                        let call_gas_limit = interpreter.gas.remaining();
                        syscall_gas!(interpreter, call_gas_limit);

                        debug!("> Call context:");
                        debug!("  - Caller: {}", interpreter.contract.target_address);
                        debug!("  - Target Address: {}", addr);
                        debug!("  - Value: {}", value);
                        debug!("  - Calldata: {:?}", calldata);
                        return Ok(InterpreterAction::Call {
                            inputs: Box::new(CallInputs {
                                input: calldata,
                                gas_limit: call_gas_limit,
                                target_address: addr,
                                bytecode_address: addr,
                                caller: interpreter.contract.target_address,
                                value: CallValue::Transfer(U256::from(value)),
                                scheme: CallScheme::Call,
                                is_static: false,
                                is_eof: false,
                                return_memory_offset: 0..0, // handled with `returned_data_destiny`
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
                        let data_slice = emu
                            .cpu
                            .bus
                            .get_dram_slice(data_ptr..(data_ptr + data_size))
                            .unwrap_or(&mut []);
                        let data = data_slice.to_vec();

                        // Read topics
                        let topics_start = topics_ptr;
                        let topics_end = topics_ptr + topics_size * 32;
                        let topics_slice = emu
                            .cpu
                            .bus
                            .get_dram_slice(topics_start..topics_end)
                            .unwrap_or(&mut []);
                        let topics = topics_slice
                            .chunks(32)
                            .map(B256::from_slice)
                            .collect::<Vec<B256>>();

                        host.log(Log::new_unchecked(
                            interpreter.contract.target_address,
                            topics,
                            data.into(),
                        ));
                    }
                }
            }
            Ok(_) => {
                trace!("Successful instruction at PC: {:#x}", emu.cpu.pc);
                continue;
            }
            Err(e) => {
                debug!("Execution error: {:#?}", e);
                syscall_gas!(interpreter, r55_gas_used(&emu.cpu.inst_counter));
                return return_revert(interpreter, interpreter.gas.spent());
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
