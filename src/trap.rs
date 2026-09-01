//! Kernel trap handlers.

/// Traps from the supervisor are handled here.
///
/// When a trap occurs in supervisor mode, the CPU jumps to `kernelvec` which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn kerneltrap() {}

/// Traps from the user are handled here.
///
/// When a trap occurs in user mode, the CPU jumps to `uservec` which will in turn call this
/// function to handle the trap.
#[unsafe(no_mangle)]
extern "C" fn usertrap() {}
