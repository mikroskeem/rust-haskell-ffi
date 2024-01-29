use std::ffi::{c_char, c_int};

extern "C" {
    // extern void hs_init     (int *argc, char **argv[])
    fn hs_init(argc: c_int, argv: *const *const c_char);
    // extern void hs_exit     (void);
    fn hs_exit();

    fn fibonacci_hs(a1: i32) -> i32;
}

struct HaskellRuntime {}

impl HaskellRuntime {
    fn new() {
        let args = std::ptr::null();
        unsafe { hs_init(0, args) }
    }
}

impl Drop for HaskellRuntime {
    fn drop(&mut self) {
        unsafe { hs_exit() }
    }
}

fn main() {
    let _ = HaskellRuntime::new();
    println!("fib(10) = {}", unsafe { fibonacci_hs(10) });
}
