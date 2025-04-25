#![no_std]
#![no_main]

extern crate alloc;
use core::default::Default;
use core::str::FromStr;

use alloc::vec;
use contract_derive::{contract, interface, payable, storage, Error, Event};
use eth_riscv_runtime::{block::{chain_id, timestamp}, types::*, *};

use alloy_core::primitives::{address, keccak256 as alloy_keccak, Address, Bytes, B256, U256, U8};
use erc20::IERC20;

use crate::{math::{mul_div, sqrt, MathError}, IUniswapV2Callee, IUniswapV2Factory};

// -- EVENTS -------------------------------------------------------------------
#[derive(Event)]
pub struct Transfer {
    #[indexed]
    pub from: Address,
    #[indexed]
    pub to: Address,
    pub value: U256,
}

#[derive(Event)]
pub struct Approval {
    #[indexed]
    pub owner: Address,
    #[indexed]
    pub spender: Address,
    pub value: U256,
}

#[derive(Event)]
pub struct Mint {
    #[indexed]
    pub sender: Address,
    pub amount0: U256,
    pub amount1: U256,
}

#[derive(Event)]
pub struct Burn {
    #[indexed]
    pub sender: Address,
    pub amount0: U256,
    pub amount1: U256,
    #[indexed]
    pub to: Address,
}

#[derive(Event)]
pub struct Swap {
    #[indexed]
    pub sender: Address,
    pub amount0_in: U256,
    pub amount1_in: U256,
    pub amount0_out: U256,
    pub amount1_out: U256,
    #[indexed]
    pub to: Address,
}

#[derive(Event)]
pub struct Sync {
    pub reserve0: U256,
    pub reserve1: U256,
}

// -- ERRORS -------------------------------------------------------------------
#[derive(Error)]
pub enum UniswapV2PairError {
    Locked,
    FailedCallback,
    OnlyFactory,
    InsufficientOutputAmount,
    InsufficientLiquidity,
    InvalidTo,
    InsufficientInputAmount,
    MathError,
    Overflow,
    InsufficientLiquidityMinted,
    InsufficientLiquidityBurned,
    TransferFailed,
    Expired,
    InvalidSignature,
    K,
}

impl From<MathError> for UniswapV2PairError {
    fn from(_: MathError) -> Self {
        Self::MathError
    }
}

// -- CONTRACT -----------------------------------------------------------------
#[storage]
pub struct UniswapV2Pair {
    // ERC20 storage
    total_supply: Slot<U256>,
    balance_of: Mapping<Address, Slot<U256>>,
    allowance: Mapping<Address, Mapping<Address, Slot<U256>>>,
    // TODO: handle string storage to support ERC20 metadata
    // name: Slot<String>,
    // symbol: Slot<String>,
    // decimals: Slot<u8>,

    // Domain separator storage (for EIP-712)
    domain_separator: Slot<B256>,
    nonces: Mapping<Address, Slot<U256>>,

    // Pair storage
    factory: Slot<Address>,
    token0: Slot<Address>,
    token1: Slot<Address>,

    reserve0: Slot<U256>,
    reserve1: Slot<U256>,
    last_block_at: Slot<U256>,

    price0_cumulative_last: Slot<U256>,
    price1_cumulative_last: Slot<U256>,
    k_last: Slot<U256>,

    lock: Lock<UniswapV2PairError>,
}

pub const MINIMUM_LIQUIDITY: U256 = U256::from_limbs([1000, 0, 0, 0]); // 1_000

// keccak256("Permit(address owner,address spender,uint256 value,uint256 nonce,uint256 deadline)");
pub const PERMIT_TYPEHASH: [u8; 32] = [
    0x6e, 0x71, 0xed, 0xae, 0x12, 0xb1, 0xb9, 0x7f, 0x4d, 0x1f, 0x60, 0x37, 0x0f, 0xef, 0x10, 0x10,
    0x5f, 0xa2, 0xfa, 0xae, 0x01, 0x26, 0x11, 0x4a, 0x16, 0x9c, 0x64, 0x84, 0x5d, 0x61, 0x26, 0xc9,
];

#[contract]
impl UniswapV2Pair {
    // -- CONSTRUCTOR ----------------------------------------------------------
    pub fn new() -> Self {
        let mut pair = UniswapV2Pair::default();

        // Set factory as deployer
        pair.factory.write(msg_sender());

        // // Calculate domain separator for EIP-712
        // // TODO: implement a user-frendly version of keccack256 -> it should use `Bytes` rather than a raw pointer
        // let domain_separator = alloy_core::primitives::keccak256(
        //     (
        //         alloy_keccak("EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)"),
        //         alloy_keccak(Bytes::from_str("Uniswap V2").unwrap()),
        //         alloy_keccak(Bytes::from_str("1").unwrap()),
        //         chain_id(),
        //         pair.address(),
        //     ).abi_encode()
        // );

        pair
    }

    pub fn initialize(
        &mut self,
        token0: Address,
        token1: Address,
    ) -> Result<(), UniswapV2PairError> {
        if msg_sender() != self.factory.read() {
            return Err(UniswapV2PairError::OnlyFactory);
        }

        self.token0.write(token0);
        self.token1.write(token1);

        Ok(())
    }

    // -- ERC20 - STATE MODIFYING FUNCTIONS ---------------------------------------------------
    fn _mint(&mut self, to: Address, value: U256) {
        self.total_supply += value;
        self.balance_of[to] += value;

        log::emit(Transfer::new(Address::ZERO, to, value));
    }

    fn _burn(&mut self, from: Address, value: U256) {
        self.balance_of[from] -= value;
        self.total_supply -= value;

        log::emit(Transfer::new(from, Address::ZERO, value));
    }

    fn _approve(&mut self, owner: Address, spender: Address, value: U256) {
        self.allowance[owner][spender].write(value);

        log::emit(Approval::new(owner, spender, value));
    }

    fn _transfer(&mut self, from: Address, to: Address, value: U256) {
        self.balance_of[from] -= value;
        self.balance_of[to] += value;

        log::emit(Transfer::new(from, to, value));
    }

    pub fn approve(&mut self, spender: Address, value: U256) -> bool {
        self._approve(msg_sender(), spender, value);
        true
    }

    pub fn transfer(&mut self, to: Address, value: U256) -> bool {
        self._transfer(msg_sender(), to, value);
        true
    }

    pub fn transfer_from(&mut self, from: Address, to: Address, value: U256) -> bool {
        let msg_sender = msg_sender();
        let allowance = self.allowance[from][msg_sender].read();

        if allowance != U256::MAX {
            self.allowance[from][msg_sender].write(allowance - value);
        }

        self._transfer(from, to, value);
        true
    }

    // TODO: figure out how to use U8 as apparently it doesn't impl `SolValue`:
    // https://github.com/alloy-rs/core/blob/cd4b8d5e77e38d71534742fa4fb0505505b2ca2d/crates/sol-types/src/types/value.rs#L221
    //
    // pub fn permit(&mut self, owner: Address, spender: Address, value: U256, deadline: U256, _v: U8, _r: B256, _s: B256) -> Result<(), UniswapV2PairError> {
    //     if deadline < timestamp() {
    //         return Err(UniswapV2PairError::Expired)
    //     }
    //
    //     let nonce = self.nonces[owner].read() + U256::from(1);
    //     self.nonces[owner].write(nonce);
    //
    //     let _digest = alloy_core::primitives::keccak256((
    //         vec![0x19, 0x01],
    //         self.domain_separator.read(),
    //         alloy_core::primitives::keccak256((PERMIT_TYPEHASH, owner, spender, value, nonce, deadline).abi_encode())
    //     ).abi_encode_packed());
    //
    //     // TODO: Implement ECDSA verification (ecrecover)
    //     let signer = Address::ZERO;
    //
    //     if signer == Address::ZERO || signer != owner {
    //         return Err(UniswapV2PairError::InvalidSignature)
    //     }
    //     self._approve(owner, spender, value);
    //
    //     Ok(())
    // }

    // -- ERC20 - READ-ONLY FUNCTIONS ---------------------------------------------------
    pub fn total_supply(&self) -> U256 {
        self.total_supply.read()
    }

    pub fn balance_of(&self, owner: Address) -> U256 {
        self.balance_of[owner].read()
    }

    pub fn allowance(&self, owner: Address, spender: Address) -> U256 {
        self.allowance[owner][spender].read()
    }

    // TODO: use Lock in the state modifying pair fns

    // -- AMM PAIR - STATE MODIFYING FUNCTIONS -----------------------------------------------
    fn _update(&mut self, balance0: U256, balance1: U256, reserve0: U256, reserve1: U256) {
        let block_timestamp = timestamp();
        let elapsed = block_timestamp - self.last_block_at.read();

        // Update price accumulators
        if elapsed != U256::ZERO && reserve0 != U256::ZERO && reserve1 != U256::ZERO {
            self.price0_cumulative_last +=
                mul_div(reserve1, U256::from(elapsed), reserve0).unwrap_or(U256::ZERO);
            self.price1_cumulative_last +=
                mul_div(reserve0, U256::from(elapsed), reserve1).unwrap_or(U256::ZERO);
        }

        // Update reserves
        self.reserve0.write(balance0);
        self.reserve1.write(balance1);
        self.last_block_at.write(block_timestamp);

        log::emit(Sync::new(balance0, balance1));
    }

    fn _mint_fee(&mut self, reserve0: U256, reserve1: U256) -> Result<bool, UniswapV2PairError> {
        let factory_contract = IUniswapV2Factory::new(self.factory.read()).with_ctx(&mut *self);
        let fee_to = factory_contract.fee_to().unwrap_or(Address::ZERO);

        let is_fee_active = fee_to != Address::ZERO;
        let k_last = self.k_last.read();

        // If fee is on, mint liquidity equivalent to 1/6th of the growth in sqrt(k)
        if is_fee_active {
            if k_last != U256::ZERO {
                let root_k = sqrt(reserve0 * reserve1);
                let root_k_last = sqrt(k_last);

                if root_k > root_k_last {
                    let liquidity = mul_div(
                        self.total_supply.read(),
                        root_k - root_k_last,
                        root_k * U256::from(5) + root_k_last,
                    )?;

                    if liquidity != U256::ZERO {
                        self._mint(fee_to, liquidity);
                    }
                }
            }
        } else if k_last != U256::ZERO {
            self.k_last.write(U256::ZERO);
        }

        Ok(is_fee_active)
    }

    pub fn mint(&mut self, to: Address) -> Result<U256, UniswapV2PairError> {
        let _guard = self.lock.acquire(UniswapV2PairError::Locked)?;

        let (reserve0, reserve1, _) = self.get_reserves();
        let is_fee_active = self._mint_fee(reserve0, reserve1)?;

        let token0 = IERC20::new(self.token0.read()).with_ctx(&mut *self);
        let token1 = IERC20::new(self.token1.read()).with_ctx(&mut *self);

        // Get total supply, current balances, and compute the amounts
        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);
        let amount0 = balance0 - reserve0;
        let amount1 = balance1 - reserve1;

        let total_supply = self.total_supply.read();

        let liquidity = {
            if total_supply == U256::ZERO {
                // Permanently lock the first `MINIMUM_LIQUIDITY` tokens
                self._mint(Address::ZERO, MINIMUM_LIQUIDITY);
                sqrt(amount0 * amount1) - MINIMUM_LIQUIDITY
            } else {
                core::cmp::min(
                    mul_div(amount0, total_supply, reserve0)?,
                    mul_div(amount1, total_supply, reserve1)?,
                )
            }
        };

        if liquidity == U256::ZERO {
            return Err(UniswapV2PairError::InsufficientLiquidityMinted);
        }

        // Update the AMM reserves
        self._mint(to, liquidity);
        self._update(balance0, balance1, reserve0, reserve1);
        if is_fee_active {
            self.k_last.write(balance0 * balance1)
        };

        // Emit the event and return the liquidity
        log::emit(Mint::new(msg_sender(), amount0, amount1));

        Ok(liquidity)
    }

    pub fn burn(&mut self, to: Address) -> Result<(U256, U256), UniswapV2PairError> {
        let _guard = self.lock.acquire(UniswapV2PairError::Locked)?;

        let (reserve0, reserve1, _) = self.get_reserves();
        let is_fee_active = self._mint_fee(reserve0, reserve1)?;

        let token0 = IERC20::new(self.token0.read()).with_ctx(&mut *self);
        let token1 = IERC20::new(self.token1.read()).with_ctx(&mut *self);

        // Get total supply, current balances, and compute the amounts
        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);

        let liquidity = self.balance_of[self.address()].read();
        let total_supply = self.total_supply.read();

        let amount0 = mul_div(liquidity, balance0, total_supply)?;
        let amount1 = mul_div(liquidity, balance1, total_supply)?;

        if amount0 <= U256::ZERO || amount1 <= U256::ZERO {
            return Err(UniswapV2PairError::InsufficientLiquidityBurned);
        }

        // Burn the LP tokens and transfer the amounts back
        self._burn(self.address(), liquidity);

        self._transfer(token0.address(), to, amount0);
        self._transfer(token1.address(), to, amount1);

        // Update the AMM reserves
        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);

        self._update(balance0, balance1, reserve0, reserve1);
        if is_fee_active {
            self.k_last.write(balance0 * balance1)
        };

        // Emit the event and return the token amounts
        log::emit(Burn::new(msg_sender(), amount0, amount1, to));

        Ok((amount0, amount1))
    }

    pub fn swap(
        &mut self,
        amount0_out: U256,
        amount1_out: U256,
        to: Address,
        data: Bytes,
    ) -> Result<(), UniswapV2PairError> {
        let _guard = self.lock.acquire(UniswapV2PairError::Locked)?;

        if amount0_out == U256::ZERO && amount1_out == U256::ZERO {
            return Err(UniswapV2PairError::InsufficientOutputAmount);
        }

        let (reserve0, reserve1, _) = self.get_reserves();
        if amount0_out >= reserve0 || amount1_out >= reserve1 {
            return Err(UniswapV2PairError::InsufficientLiquidity);
        }

        let token0 = IERC20::new(self.token0.read()).with_ctx(&mut *self);
        let token1 = IERC20::new(self.token1.read()).with_ctx(&mut *self);

        if to == token0.address() || to == token1.address() {
            return Err(UniswapV2PairError::InvalidTo);
        }

        // Optimistically transfer tokens
        if amount0_out > U256::ZERO {
            self._transfer(token0.address(), to, amount0_out);
        }

        if amount1_out > U256::ZERO {
            self._transfer(token1.address(), to, amount1_out);
        }

        // Swap callback
        if !data.is_empty() {
            IUniswapV2Callee::new(to)
                .with_ctx(&mut *self)
                .uniswap_v2_call(msg_sender(), amount0_out, amount1_out, data)
                .ok_or(UniswapV2PairError::FailedCallback)?;
        }

        // Get balances after transfers and possibly callback
        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);

        // Calculate amounts in
        let amount0_in = if balance0 > reserve0 - amount0_out {
            balance0 - (reserve0 - amount0_out)
        } else {
            U256::ZERO
        };

        let amount1_in = if balance1 > reserve1 - amount1_out {
            balance1 - (reserve1 - amount1_out)
        } else {
            U256::ZERO
        };

        if amount0_in == U256::ZERO && amount1_in == U256::ZERO {
            return Err(UniswapV2PairError::InsufficientInputAmount);
        }

        // Verify `K` constant (with fee)
        let balance0_adjusted = balance0 * U256::from(1000) - amount0_in * U256::from(3);
        let balance1_adjusted = balance1 * U256::from(1000) - amount1_in * U256::from(3);

        if balance0_adjusted * balance1_adjusted < reserve0 * reserve1 * U256::from(1_000_000) {
            return Err(UniswapV2PairError::K);
        }

        // Update reserves
        self._update(balance0, balance1, reserve0, reserve1);

        log::emit(Swap::new(msg_sender(), amount0_in, amount1_in, amount0_out, amount1_out, to));

        Ok(())
    }

    pub fn skim(&mut self, to: Address) -> Result<(), UniswapV2PairError> {
        let _guard = self.lock.acquire(UniswapV2PairError::Locked)?;

        let token0 = IERC20::new(self.token0.read()).with_ctx(&mut *self);
        let token1 = IERC20::new(self.token1.read()).with_ctx(&mut *self);

        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);

        self._transfer(token0.address(), to, balance0 - self.reserve0.read());
        self._transfer(token1.address(), to, balance1 - self.reserve1.read());

        Ok(())
    }

    pub fn sync(&mut self) -> Result<(), UniswapV2PairError> {
        let _guard = self.lock.acquire(UniswapV2PairError::Locked)?;

        let token0 = IERC20::new(self.token0.read()).with_ctx(&mut *self);
        let token1 = IERC20::new(self.token1.read()).with_ctx(&mut *self);

        let balance0 = token0.balance_of(self.address()).unwrap_or(U256::ZERO);
        let balance1 = token1.balance_of(self.address()).unwrap_or(U256::ZERO);

        self._update(balance0, balance1, self.reserve0.read(), self.reserve1.read());

        Ok(())
    }

    // -- AMM PAIR - READ-ONLY FUNCTIONS -----------------------------------------------------
    pub fn factory(&self) -> Address {
        self.factory.read()
    }

    pub fn token0(&self) -> Address {
        self.token0.read()
    }

    pub fn token1(&self) -> Address {
        self.token1.read()
    }

    pub fn get_reserves(&self) -> (U256, U256, U256) {
        (
            self.reserve0.read(),
            self.reserve1.read(),
            self.last_block_at.read(),
        )
    }
}
