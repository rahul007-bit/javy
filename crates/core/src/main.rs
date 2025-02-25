use javy::Runtime;
use once_cell::sync::OnceCell;
use std::io::{self, Read};
use std::slice;
use std::str;
use std::string::String;

mod execution;
mod runtime;

const FUNCTION_MODULE_NAME: &str = "function.mjs";

static mut COMPILE_SRC_RET_AREA: [u32; 2] = [0; 2];

static mut RUNTIME: OnceCell<Runtime> = OnceCell::new();
static mut BYTECODE: OnceCell<Vec<u8>> = OnceCell::new();

#[export_name = "wizer.initialize"]
pub extern "C" fn init() {
    let _wasm_ctx = WasmCtx::new();

    let runtime = runtime::new_runtime().unwrap();

    let mut contents = String::new();
    io::stdin().read_to_string(&mut contents).unwrap();
    let bytecode = runtime
        .context()
        .compile_module("function.mjs", &contents)
        .unwrap();

    unsafe {
        RUNTIME.set(runtime).unwrap();
        BYTECODE.set(bytecode).unwrap();
    }
}

fn main() {
    let bytecode = unsafe { BYTECODE.take().unwrap() };
    let runtime = unsafe { RUNTIME.take().unwrap() };
    execution::run_bytecode(&runtime, &bytecode);
}

#[export_name = "run_entrypoint"]
pub unsafe fn run_entrypoint(input_ptr: *mut u8, input_len: usize) -> *const u32 {
    let runtime = unsafe { RUNTIME.take().unwrap() };
    let bytecode = unsafe { BYTECODE.take().unwrap() };
    let input = str::from_utf8(unsafe { slice::from_raw_parts(input_ptr, input_len) }).unwrap();
    let result = execution::invoke_entrypoint(&runtime, &bytecode, input.to_string()).unwrap();
    let result_len = result.len();
    let result_ptr = Box::leak(result.into_boxed_slice()).as_ptr();
    COMPILE_SRC_RET_AREA[0] = result_ptr as u32;
    COMPILE_SRC_RET_AREA[1] = result_len.try_into().unwrap();
    COMPILE_SRC_RET_AREA.as_ptr()
}

// Removed in post-processing.
/// Evaluates QuickJS bytecode and invokes the exported JS function name.
///
/// # Safety
///
/// * `fn_name_ptr` must reference a UTF-8 string with `fn_name_size` byte
///   length.
#[export_name = "javy.invoke"]
pub unsafe extern "C" fn invoke(fn_name_ptr: *mut u8, fn_name_size: usize) {
    let _wasm_ctx = WasmCtx::new();

    let js_fn_name = str::from_utf8_unchecked(slice::from_raw_parts(fn_name_ptr, fn_name_size));
    let runtime = unsafe { RUNTIME.take().unwrap() };
    execution::invoke_function(&runtime, FUNCTION_MODULE_NAME, js_fn_name);
}

// RAII abstraction for calling Wasm ctors and dtors for exported non-main functions.
struct WasmCtx;

impl WasmCtx {
    #[must_use = "Failing to assign the return value will result in the wasm dtors being run immediately"]
    fn new() -> Self {
        unsafe { __wasm_call_ctors() };
        Self
    }
}

impl Drop for WasmCtx {
    fn drop(&mut self) {
        unsafe { __wasm_call_dtors() };
    }
}

extern "C" {
    // `__wasm_call_ctors` is generated by `wasm-ld` and invokes all of the global constructors.
    // In a Rust bin crate, the `_start` function will invoke this implicitly but no other exported
    // Wasm functions will invoke this.
    // If this is not invoked, access to environment variables and directory preopens will not be
    // available.
    // This should only be invoked at the start of exported Wasm functions that are not the `main`
    // function.
    // References:
    // - [Rust 1.67.0 stopped initializing the WASI environment for exported functions](https://github.com/rust-lang/rust/issues/107635)
    // - [Wizer header in Fastly's JS compute runtime](https://github.com/fastly/js-compute-runtime/blob/main/runtime/js-compute-runtime/third_party/wizer.h#L92)
    fn __wasm_call_ctors();

    fn __wasm_call_dtors();
}
