# R55

R55 is an experimental Ethereum Execution Environment that seamlessly
integrates RISCV smart contracts alongside traditional EVM smart contracts.
This dual support operates over the same Ethereum state, and communication
happens via ABI-encoded calls.

On the high level, R55 enables the use of pure Rust smart contracts, opening
the door for a vast Rust developer community to engage in Ethereum development
with minimal barriers to entry, and increasing language and compiler diversity.

On the low level, RISCV code allows for optimization opportunities distinct
from the EVM, including the use of off-the-shelf ASICs. This potential for
performance gains can be particularly advantageous in specialized domains.

# Off-the-shelf tooling

R55 relies on standard tooling that programmers are used to, such as Rust,
Cargo and LLVM. This directly enables tooling such as linters, static
analyzers, testing, fuzzing, and formal verification tools to be applied to
these smart contracts without extra development and research.

# Pure & Clean Rust Smart Contracts

Differently from other platforms that offer Rust smart contracts, R55 lets the
user code in [no\_std] Rust without weird edges. The code below implements a
basic ERC20 token with infinite minting for testing.
Because the `struct` and `impl` are just Rust code, the user can write normal
tests and run them natively (as long as they don't need Ethereum host
functions).  Note that [alloy-rs](https://github.com/alloy-rs/) types work
out-of-the-box.

```rust
#![no_std]
#![no_main]

use core::default::Default;

use contract_derive::contract;
use eth_riscv_runtime::types::Mapping;

use alloy_core::primitives::Address;

#[derive(Default)]
pub struct ERC20 {
    balance: Mapping<Address, u64>,
}

#[contract]
impl ERC20 {
    pub fn balance_of(&self, owner: Address) -> u64 {
        self.balance.read(owner)
    }

    pub fn transfer(&self, from: Address, to: Address, value: u64) {
        let from_balance = self.balance.read(from);
        let to_balance = self.balance.read(to);

        if from == to || from_balance < value {
            revert();
        }

        self.balance.write(from, from_balance - value);
        self.balance.write(to, to_balance + value);
    }

    pub fn mint(&self, to: Address, value: u64) {
        let to_balance = self.balance.read(to);
        self.balance.write(to, to_balance + value);
    }
}
```

The macro `#[contract]` above is the only special treatment the user needs to
apply to their code. Specifically, it is responsible for the init code
(deployer), and for creating the function dispatcher based on the given
methods.
Note that Rust `pub` methods are exposed as public functions in the deployed
contract, similarly to Solidity's `public` functions.

# Client Integration

R55 is a fork of [revm](https://github.com/bluealloy/revm) without any API
changes.  Therefore it can be used seamlessly in Anvil/Reth to deploy a
testnet/network with support to RISCV smart contracts.
Nothing has to be changed in how transactions are handled or created.

# Relevant Links

- [revm-R55](https://github.com/r0qs/revm)
- [rvemu-R55](https://github.com/lvella/rvemu)
- [R55 Ethereum Runtime](https://github.com/leonardoalt/r55/tree/main/eth-riscv-runtime)
- [R55 Compiler](https://github.com/leonardoalt/r55/tree/main/r55)

# Prerequisites

## macOS

```shell
brew install riscv-gnu-toolchain gettext
```

# Test

The [R55](https://github.com/leonardoalt/r55/tree/main/r55) crate has a binary
that puts everything together in an end-to-end PoC, compiling the
[erc20](https://github.com/leonardoalt/r55/tree/main/erc20) contract, deploying
it to an internal instance of [revm-r55](https://github.com/r0qs/revm), and
running two transactions on it, first a `mint` then a `balance_of` check.

You'll need to install Rust's RISCV toolchain:

```console
$ rustup install nightly-2024-02-01-x86_64-unknown-linux-gnu
```

Now run:

```console
$ cargo run
...
Compiling deploy: erc20
Cargo command completed successfully
Deployed at addr: 0x522b3294e6d06aa25ad0f1b8891242e335d3b459
Tx result: 0x
Tx result: 0x000000000000000000000000000000000000000000000000000000000000002a
```

First R55 compiles the runtime RISCV-ELF binary that will be deployed. This is
needed to also compile the initcode RISCV-ELF binary that runs the constructor
and creates the contract.
The `mint` function has no return values, seen in `Tx result: 0x`. We minted 42
tokens to our test account in the first transaction, and we can see in the
second transaction that indeed the balance is 42 (0x2a).

# Architecture

The compiler uses `rustc`, `llvm`,
[eth-riscv-syscalls](https://github.com/leonardoalt/r55/tree/main/eth-riscv-syscalls),
[eth-riscv-runtime](https://github.com/leonardoalt/r55/tree/main/eth-riscv-runtime)
and [riscv-rt](https://github.com/rust-embedded/riscv/tree/master/riscv-rt) to
compile and link ELF binaries with low-level syscalls to be executed by
[rvemu-r55](https://github.com/lvella/rvemu):

```mermaid
graph TD;
    RustContract[Rust contract] --> CompiledContract[compiled contract]
    rustc --> CompiledContract
    llvm --> CompiledContract
    EthRiscVSyscalls[eth-riscv-syscalls] --> CompiledContract
    EthRiscVRuntime1[eth-riscv-runtime] --> CompiledContract
    CompiledContract --> LinkedRuntimeBytecode[linked runtime bytecode]
    EthRiscVRuntime2[eth-riscv-runtime] --> LinkedRuntimeBytecode
    riscv_rt[riscv-rt] --> LinkedRuntimeBytecode
    LinkedRuntimeBytecode --> LinkedInitBytecode[linked init bytecode]
    EthRiscVRuntime3[eth-riscv-runtime] --> LinkedInitBytecode
```

The execution environment depends on [revm](https://github.com/bluealloy/revm),
and relies on the [rvemu-r55](https://github.com/lvella/rvemu) RISCV
interpreter and
[eth-riscv-runtime](https://github.com/leonardoalt/r55/tree/main/eth-riscv-runtime).

```mermaid
graph TD;
    revm --> revm-r55
    rvemu-r55 --> revm-r55
    eth-riscv-runtime --> revm-r55
```
