use alloy_core::primitives::Keccak256;
use eth_riscv_interpreter::setup_from_elf;
use eth_riscv_syscalls::Syscall;
use revm::{
    handler::register::EvmHandler,
    interpreter::{
        CallInputs, CallScheme, CallValue, Host, InstructionResult, Interpreter, InterpreterAction,
        InterpreterResult, SharedMemory,
    },
    primitives::{address, Address, Bytes, ExecutionResult, Output, TransactTo, U256},
    Database, Evm, Frame, FrameOrResult, InMemoryDB,
};
use rvemu::{emulator::Emulator, exception::Exception};
use std::{cell::RefCell, ops::Range, rc::Rc};

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

pub fn deploy_contract(db: &mut InMemoryDB, bytecode: Bytes) -> Address {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("0000000000000000000000000000000000000001");
            tx.transact_to = TransactTo::Create;
            tx.data = bytecode;
            tx.value = U256::from(0);
        })
        .append_handler_register(handle_register_riscv)
        .build();
    evm.cfg_mut().limit_contract_code_size = Some(usize::MAX);

    let result = evm.transact_commit().unwrap();

    match result {
        ExecutionResult::Success {
            output: Output::Create(_value, Some(addr)),
            ..
        } => {
            println!("Deployed at addr: {:?}", addr);
            addr
        }
        result => panic!("Unexpected result: {:?}", result),
    }
}

pub fn run_tx_prolog(db: &mut InMemoryDB, addr: &Address, calldata: Vec<u8>) {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("0000000000000000000000000000000000000007");
            tx.transact_to = TransactTo::Call(*addr);
            tx.data = calldata.into();
            tx.value = U256::from(0);
        })
        .append_handler_register(handle_register_prolog)
        .build();

    let result = evm.transact_commit().unwrap();

    match result {
        ExecutionResult::Success {
            output: Output::Call(value),
            ..
        } => println!("Tx result: {:?}", value),
        result => panic!("Unexpected result: {:?}", result),
    };
}

pub fn run_tx(db: &mut InMemoryDB, addr: &Address, calldata: Vec<u8>) {
    let mut evm = Evm::builder()
        .with_db(db)
        .modify_tx_env(|tx| {
            tx.caller = address!("0000000000000000000000000000000000000007");
            tx.transact_to = TransactTo::Call(*addr);
            tx.data = calldata.into();
            tx.value = U256::from(0);
        })
        .append_handler_register(handle_register_riscv)
        .build();

    let result = evm.transact_commit().unwrap();

    match result {
        ExecutionResult::Success {
            output: Output::Call(value),
            ..
        } => println!("Tx result: {:?}", value),
        result => panic!("Unexpected result: {:?}", result),
    };
}

#[derive(Debug, Clone, Default)]
struct Prolog {
    program: String,
    calldata: String,
}

impl Prolog {
    pub fn new(program: &[u8], calldata: &[u8]) -> Self {
        let program = String::from_utf8_lossy(program).to_string();
        let program: String = program.chars().filter(|&c| c != '\0').collect();
        let calldata = String::from_utf8_lossy(calldata).to_string();
        Self { program, calldata }
    }
}

#[derive(Debug, Clone, Default)]
struct PrologEmu {
    emu: Prolog,
    _returned_data_destiny: Option<Range<u64>>,
}

fn prolog_context(frame: &Frame) -> Option<PrologEmu> {
    let interpreter = frame.interpreter();

    let Some((0xF7, bytecode)) = interpreter.bytecode.split_first() else {
        return None;
    };
    let emu = Prolog::new(bytecode, &interpreter.contract.input);
    Some(PrologEmu {
        emu,
        _returned_data_destiny: None,
    })
}

fn invoke_scryer_prolog(program: String, calldata: String) {
    let module = include_bytes!("../eth.pl");
    let module = String::from_utf8_lossy(module).to_string();

    let goal = format!("main({calldata}, S, R).");
    let full_program = format!("\n% module\n{module}\n% end module\n{program}\n% end program\n");
    //println!("Prolog Program:\n{full_program}\nEnd Program");

    let mut temp_file = NamedTempFile::new().expect("Failed to create temporary file");
    temp_file
        .write_all(full_program.as_bytes())
        .expect("Failed to write Prolog program to file");

    let temp_file_path = temp_file.path();

    let _output = Command::new("scryer-prolog")
        .arg(temp_file_path)
        .arg("--goal")
        .arg(goal)
        .output()
        .expect("Failed to execute Scryer Prolog");

    /*
    if output.status.success() {
        println!(
            "Prolog Output:\n{}",
            String::from_utf8_lossy(&output.stdout)
        );
    } else {
        eprintln!(
            "Prolog Error:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    */
}
use tokio::sync::Notify;
async fn start_prolog_host(
    interpreter: &mut Interpreter,
    host: &mut dyn Host,
    notify: Arc<Notify>,
) -> InterpreterAction {
    let hostname = "127.0.0.1";
    let port = 12345;
    let address = format!("{hostname}:{port}");

    let listener = TcpListener::bind(&address).await.expect("Failed to bind");
    //println!("[INFO] Server listening on {address}");

    notify.notify_one();

    loop {
        let (socket, addr) = listener
            .accept()
            .await
            .expect("Failed to accept connection");

        //println!("[INFO] Connection established with {addr}");
        let res = handle_client(socket, interpreter, host).await;
        match res {
            Ok(res) => {
                if !res.is_empty() {
                    return InterpreterAction::Return {
                        result: InterpreterResult {
                            result: InstructionResult::Return,
                            output: res.into(),
                            gas: interpreter.gas, // FIXME: gas is not correct
                        },
                    };
                }
            }
            Err(e) => {
                panic!("[ERROR] Error handling client {addr}: {e}");
            }
        }
    }
}

async fn execute_prolog(prolog_emu: &mut PrologEmu, notify: Arc<Notify>) {
    notify.notified().await;
    let emu = &mut prolog_emu.emu;

    use std::thread;

    let program_arc = Arc::new(emu.program.clone());
    let cd_arc = Arc::new(emu.calldata.clone());
    let handle = thread::spawn({
        move || {
            invoke_scryer_prolog((*program_arc).clone(), (*cd_arc).clone());
        }
    });

    handle.join().expect("Thread panicked");
}

async fn handle_client(
    socket: TcpStream,
    interpreter: &mut Interpreter,
    host: &mut dyn Host,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);

    let mut buffer = String::new();

    let mut result = Vec::new();
    loop {
        buffer.clear();
        let bytes_read = reader.read_line(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }

        let command = buffer.trim();
        let parts: Vec<&str> = command.split_whitespace().collect();

        if parts.is_empty() {
            writer.write_all(b"error.\n").await?;
            continue;
        }

        match parts[0] {
            "stop" => {
                break;
            }
            "sload" if parts.len() == 2 => {
                let key: u32 = parts[1].parse().unwrap_or(0);
                //println!("sload({key})");
                match host.sload(interpreter.contract.target_address, U256::from(key)) {
                    Some((value, _is_cold)) => {
                        writer
                            .write_all(format!("value({value}).\n").as_bytes())
                            .await?;
                    }
                    _ => {
                        //return return_revert(interpreter);
                    }
                }
            }
            "sstore" if parts.len() == 3 => {
                let key: u32 = parts[1].parse().unwrap_or(0);
                let value: u32 = parts[2].parse().unwrap_or(0);
                //println!("sstore({key}, {value})");
                host.sstore(
                    interpreter.contract.target_address,
                    U256::from(key),
                    U256::from(value),
                );
                writer.write_all(b"ok.\n").await?;
            }
            "return" if parts.len() == 2 => {
                let res: u32 = parts[1].parse().unwrap_or(0);
                result.extend(res.to_be_bytes());

                //println!("return({res})");
                writer.write_all(b"ok.\n").await?;
            }
            _ => {
                writer.write_all(b"error.\n").await?;
            }
        }
    }

    Ok(result)
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
    let emu = setup_from_elf(bytecode, &interpreter.contract.input);
    Some(RVEmu {
        emu,
        returned_data_destiny: None,
    })
}

pub fn handle_register_prolog<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
    let call_stack = Rc::<RefCell<Vec<_>>>::new(RefCell::new(Vec::new()));

    // create a prolog context on call frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.call.clone();
    handler.execution.call = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(prolog_context(frame));
        }
        result
    });

    // create a prolog context on create frame.
    let call_stack_inner = call_stack.clone();
    let old_handle = handler.execution.create.clone();
    handler.execution.create = Arc::new(move |ctx, inputs| {
        let result = old_handle(ctx, inputs);
        if let Ok(FrameOrResult::Frame(frame)) = &result {
            call_stack_inner.borrow_mut().push(prolog_context(frame));
        }
        result
    });

    handler.execution.execute_frame = Arc::new(move |frame, _memory, _instruction_table, ctx| {
        let call_stack = Rc::clone(&call_stack);
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        Ok(runtime.block_on(async move {
            let notify = Arc::new(Notify::new());
            let notify_clone = Arc::clone(&notify);
            if let Some(Some(prolog_context)) = call_stack.borrow_mut().first_mut() {
                let mut p = prolog_context.clone();
                tokio::spawn(async move {
                    execute_prolog(&mut p, notify_clone).await;
                });
            } else {
                panic!()
            }
            let host = start_prolog_host(frame.interpreter_mut(), ctx, notify);
            host.await
        }))
    });
}

pub fn handle_register_riscv<EXT, DB: Database>(handler: &mut EvmHandler<'_, EXT, DB>) {
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
            execute_riscv(riscv_context, frame.interpreter_mut(), memory, ctx)
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
) -> InterpreterAction {
    let emu = &mut rvemu.emu;
    let returned_data_destiny = &mut rvemu.returned_data_destiny;
    if let Some(destiny) = std::mem::take(returned_data_destiny) {
        let data = emu
            .cpu
            .bus
            .get_dram_slice(destiny)
            .unwrap_or_else(|e| panic!("Unable to get destiny dram slice ({e:?})"));
        data.copy_from_slice(shared_memory.slice(0, data.len()))
    }

    let return_revert = |interpreter: &mut Interpreter| {
        InterpreterAction::Return {
            result: InterpreterResult {
                result: InstructionResult::Revert,
                // return empty bytecode
                output: Bytes::new(),
                gas: interpreter.gas,
            },
        }
    };

    // Run emulator and capture ecalls
    loop {
        let run_result = emu.start();
        match run_result {
            Err(Exception::EnvironmentCallFromMMode) => {
                let t0: u64 = emu.cpu.xregs.read(5);

                let Ok(syscall) = Syscall::try_from(t0 as u8) else {
                    println!("Unhandled syscall: {:?}", t0);
                    return return_revert(interpreter);
                };

                match syscall {
                    Syscall::Return => {
                        let ret_offset: u64 = emu.cpu.xregs.read(10);
                        let ret_size: u64 = emu.cpu.xregs.read(11);
                        let data_bytes = dram_slice(emu, ret_offset, ret_size);

                        return InterpreterAction::Return {
                            result: InterpreterResult {
                                result: InstructionResult::Return,
                                output: data_bytes.to_vec().into(),
                                gas: interpreter.gas, // FIXME: gas is not correct
                            },
                        };
                    }
                    Syscall::SLoad => {
                        let key: u64 = emu.cpu.xregs.read(10);
                        match host.sload(interpreter.contract.target_address, U256::from(key)) {
                            Some((value, _is_cold)) => {
                                emu.cpu.xregs.write(10, value.as_limbs()[0]);
                            }
                            _ => {
                                return return_revert(interpreter);
                            }
                        }
                    }
                    Syscall::SStore => {
                        let key: u64 = emu.cpu.xregs.read(10);
                        let value: u64 = emu.cpu.xregs.read(11);
                        host.sstore(
                            interpreter.contract.target_address,
                            U256::from(key),
                            U256::from(value),
                        );
                    }
                    Syscall::Call => {
                        let a0: u64 = emu.cpu.xregs.read(10);
                        let address =
                            Address::from_slice(emu.cpu.bus.get_dram_slice(a0..(a0 + 20)).unwrap());
                        let value: u64 = emu.cpu.xregs.read(11);
                        let args_offset: u64 = emu.cpu.xregs.read(12);
                        let args_size: u64 = emu.cpu.xregs.read(13);
                        let ret_offset = emu.cpu.xregs.read(14);
                        let ret_size = emu.cpu.xregs.read(15);

                        *returned_data_destiny = Some(ret_offset..(ret_offset + ret_size));

                        let tx = &host.env().tx;
                        return InterpreterAction::Call {
                            inputs: Box::new(CallInputs {
                                input: emu
                                    .cpu
                                    .bus
                                    .get_dram_slice(args_offset..(args_offset + args_size))
                                    .unwrap()
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
                        };
                    }
                    Syscall::Revert => {
                        return InterpreterAction::Return {
                            result: InterpreterResult {
                                result: InstructionResult::Revert,
                                output: Bytes::from(0u32.to_le_bytes()), //TODO: return revert(0,0)
                                gas: interpreter.gas, // FIXME: gas is not correct
                            },
                        };
                    }
                    Syscall::Caller => {
                        let caller = interpreter.contract.caller;
                        // Break address into 3 u64s and write to registers
                        let caller_bytes = caller.as_slice();
                        let first_u64 = u64::from_be_bytes(caller_bytes[0..8].try_into().unwrap());
                        emu.cpu.xregs.write(10, first_u64);
                        let second_u64 =
                            u64::from_be_bytes(caller_bytes[8..16].try_into().unwrap());
                        emu.cpu.xregs.write(11, second_u64);
                        let mut padded_bytes = [0u8; 8];
                        padded_bytes[..4].copy_from_slice(&caller_bytes[16..20]);
                        let third_u64 = u64::from_be_bytes(padded_bytes);
                        emu.cpu.xregs.write(12, third_u64);
                    }
                    Syscall::Keccak256 => {
                        let ret_offset: u64 = emu.cpu.xregs.read(10);
                        let ret_size: u64 = emu.cpu.xregs.read(11);
                        let data_bytes = dram_slice(emu, ret_offset, ret_size);

                        let mut hasher = Keccak256::new();
                        hasher.update(data_bytes);
                        let hash: [u8; 32] = hasher.finalize().into();

                        // Write the hash to the emulator's registers
                        emu.cpu
                            .xregs
                            .write(10, u64::from_le_bytes(hash[0..8].try_into().unwrap()));
                        emu.cpu
                            .xregs
                            .write(11, u64::from_le_bytes(hash[8..16].try_into().unwrap()));
                        emu.cpu
                            .xregs
                            .write(12, u64::from_le_bytes(hash[16..24].try_into().unwrap()));
                        emu.cpu
                            .xregs
                            .write(13, u64::from_le_bytes(hash[24..32].try_into().unwrap()));
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
                }
            }
            _ => {
                return return_revert(interpreter);
            }
        }
    }
}

/// Returns RISC-V DRAM slice in a given size range, starts with a given offset
fn dram_slice(emu: &mut Emulator, ret_offset: u64, ret_size: u64) -> &mut [u8] {
    if ret_size != 0 {
        emu.cpu
            .bus
            .get_dram_slice(ret_offset..(ret_offset + ret_size))
            .unwrap()
    } else {
        &mut []
    }
}
