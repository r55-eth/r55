// TODO: use TLOAD/TSTORE rather than SLOAD/STORE once transient storage is implemented 
use super::*;

/// A storage primitive that implements a reentrancy guard using the RAII pattern.
///
/// `Lock<E>` wraps a `Slot<bool>` to track lock state and uses a generic error type `E` 
/// to provide type-safe error handling when attempting to acquire an already locked resource.  
///
/// The `Lock` should be initialized in the contract constructor, by calling its `fn initialize()`.
///
/// The `LockGuard` returned by `fn acquire()` automatically releases the lock when it goes out of scope,
/// ensuring the lock is dropped even if the code returns early or panics.
#[derive(Default)]
pub struct Lock<E> {
    unlocked: Slot<bool>,
    _pd: PhantomData<E>,
}

impl<E> StorageLayout for Lock<E> {
    fn allocate(first: u64, second: u64, third: u64, fourth: u64) -> Self {
        Self {
            unlocked: Slot::allocate(first, second, third, fourth),
            _pd: PhantomData,
        }
    }
}

impl<E> Lock<E> {
    /// Initialize a new lock in the unlocked state. Should only be created in the constructor.
    pub fn initialize(&mut self) {
        self.unlocked.write(true);
    }
    
    /// Attempts to acquire the lock, returning a guard that releases the lock when dropped.
    /// When unable to acquire the lock, returns `locked_err`.
    pub fn acquire(&mut self, locked_err: E) -> Result<LockGuard<E>, E> {
        if !self.unlocked.read() {
            return Err(locked_err);
        }
        
        self.unlocked.write(false);
        Ok(LockGuard { slot_id: self.unlocked.id(), _pd: PhantomData })
    }
    
    /// Checks if the lock is currently unlocked
    pub fn is_unlocked(&self) -> bool {
        self.unlocked.read()
    }
}

/// A guard that manages the locking state
pub struct LockGuard<E> {
    slot_id: U256, // Store the key directly
    _pd: PhantomData<E>, 
}

impl<E> Drop for LockGuard<E> {
    fn drop(&mut self) {
        // Write `true` back directly using the stored key and the static __write method
        <Slot<bool> as StorageStorable>::__write(self.slot_id, true);
    }
}
