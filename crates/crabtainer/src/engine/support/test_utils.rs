//! Test helpers that mutate the parent process environment.
//!
//! All helpers serialize access behind a global [`ENV_LOCK`] so that tests
//! touching `CRABTAINER_HOME` never race each other. Only compiled when
//! `cfg(test)` is active.

use std::sync::Mutex;

/// Global mutex serializing all environment mutations performed by tests.
pub static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Runs `f` with `CRABTAINER_HOME` set to `home`, then removes the variable
/// again.
///
/// Serialized behind [`ENV_LOCK`]; do not set `CRABTAINER_HOME` directly in
/// tests.
pub fn with_home<T>(home: &str, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: serialized behind ENV_LOCK so no other thread touches this env var.
    unsafe { std::env::set_var("CRABTAINER_HOME", home) };
    let result = f();
    unsafe { std::env::remove_var("CRABTAINER_HOME") };
    result
}

/// Runs `f` with `CRABTAINER_HOME` removed, falling back to the default base
/// dir (`/var/lib/crabtainer`).
///
/// Serialized behind [`ENV_LOCK`]; do not set `CRABTAINER_HOME` directly in
/// tests.
pub fn without_home<T>(f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: serialized behind ENV_LOCK.
    unsafe { std::env::remove_var("CRABTAINER_HOME") };
    f()
}
