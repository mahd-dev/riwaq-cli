use std::{error::Error, ops::Index};

use serde_json::{json, Value};
use wasmer::{Exports, MemoryView, NativeFunc, WasmPtr};

pub fn call_wasm(
    exports: Exports,
    memory_view: MemoryView<u8>,
    f: String,
    args: Value,
) -> Result<Value, Box<dyn Error>> {
    let str_malloc: NativeFunc<u64, WasmPtr<u8>> = exports
        .get_native_function("str_malloc")
        .map_err(|e| dbg!(e))?;

    let mut args = serde_json::to_string(&json!({ "body": args })).map_err(|e| dbg!(e))?;
    args.push('\0');
    let args_p = str_malloc.call(args.len() as _).map_err(|e| dbg!(e))?;

    for (i, c) in args.into_bytes().iter().enumerate() {
        memory_view.index(args_p.offset() as usize + i).replace(*c);
    }

    let f: NativeFunc<WasmPtr<u8>, WasmPtr<u8>> = exports
        .get_native_function(f.as_str())
        .map_err(|e| dbg!(e))?;

    let ptr = f.call(args_p).map_err(|e| dbg!(e))?;

    Ok(serde_json::from_str::<Value>(&str_mem_read(
        &memory_view,
        ptr.offset() as usize,
    ))?)
}

pub fn str_mem_read(mem: &MemoryView<u8>, ptr: impl Into<usize>) -> String {
    let mut data: Vec<u8> = vec![];
    for v in mem[ptr.into()..].iter() {
        let v = v.get();
        if v == b'\0' {
            break;
        }
        data.push(v);
    }
    String::from_utf8_lossy(data.as_slice()).into()
}
