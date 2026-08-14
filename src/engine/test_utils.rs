use std::sync::Mutex;

pub static ENV_LOCK: Mutex<()> = Mutex::new(());

pub fn with_home<T>(home: &str, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: serialized behind ENV_LOCK so no other thread touches this env var.
    unsafe { std::env::set_var("RUSTOCKER_HOME", home) };
    let result = f();
    unsafe { std::env::remove_var("RUSTOCKER_HOME") };
    result
}

pub fn without_home<T>(f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    // SAFETY: serialized behind ENV_LOCK.
    unsafe { std::env::remove_var("RUSTOCKER_HOME") };
    f()
}
