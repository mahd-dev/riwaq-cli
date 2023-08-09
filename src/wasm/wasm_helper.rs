use std::{error::Error, ops::Index};

use serde_json::{json, Value};
use wasmer::{Exports, MemoryView, NativeFunc, WasmPtr};

use crate::sql::driver::model::{Conn, Pool, SQLFilter};

use super::wasm_loader::WasmosEnv;

pub fn ext_sql_query(env: &WasmosEnv, ptr: WasmPtr<u8>) -> WasmPtr<u8> {
    let req_str = str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize);
    let request =
        serde_json::from_str::<wasmos::sql::Select<SQLFilter>>(Box::leak(req_str.into_boxed_str()))
            .unwrap();

    let pool = env.db_pool.to_owned();
    let res = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let pool = pool.read().await.to_owned().unwrap();
            let conn = pool.conn().await.unwrap();
            conn.all(request).await.map_err(|e| e.to_string())
        })
    });
    let s = res
        .map(|r| {
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "data": r
            }))
            .unwrap()
        })
        .unwrap_or_else(|e| {
            serde_json::to_string(&serde_json::json!({
                "ok": false,
                "msg": e
            }))
            .unwrap()
        });
    let p = env
        .str_malloc
        .get_ref()
        .unwrap()
        .call(s.len() as _)
        .map_err(|e| dbg!(e))
        .unwrap();
    str_mem_write(&env.memory.get_ref().unwrap().view(), p, s).unwrap();
    p
}

pub fn ext_sql_exec(env: &WasmosEnv, ptr: WasmPtr<u8>) -> WasmPtr<u8> {
    let req_str = str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize);
    let request =
        serde_json::from_str::<wasmos::sql::SQLRequest<SQLFilter>>(req_str.as_str()).unwrap();

    let res = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let pool = env.db_pool.to_owned().read().await.to_owned().unwrap();
            let conn = pool.conn().await.unwrap();
            conn.exec(request).await.map_err(|e| e.to_string())
        })
    });
    let s = res
        .map(|r| {
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "data": r
            }))
            .unwrap()
        })
        .unwrap_or_else(|e| {
            serde_json::to_string(&serde_json::json!({
                "ok": false,
                "msg": e
            }))
            .unwrap()
        });

    let p = env
        .str_malloc
        .get_ref()
        .unwrap()
        .call(s.len() as _)
        .map_err(|e| dbg!(e))
        .unwrap();
    str_mem_write(&env.memory.get_ref().unwrap().view(), p, s).unwrap();
    p
}

pub fn ext_custom_sql_query(env: &WasmosEnv, ptr: WasmPtr<u8>) -> WasmPtr<u8> {
    let req_str = str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize);
    let request = Box::leak(req_str.into_boxed_str());

    let pool = env.db_pool.to_owned();
    let res = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let pool = pool.read().await.to_owned().unwrap();
            let conn = pool.conn().await.unwrap();
            conn.custom_query(request.to_string()).await.map_err(|e| e.to_string())
        })
    });
    let s = res
        .map(|r| {
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "data": r
            }))
            .unwrap()
        })
        .unwrap_or_else(|e| {
            serde_json::to_string(&serde_json::json!({
                "ok": false,
                "msg": e
            }))
            .unwrap()
        });
    let p = env
        .str_malloc
        .get_ref()
        .unwrap()
        .call(s.len() as _)
        .map_err(|e| dbg!(e))
        .unwrap();
    str_mem_write(&env.memory.get_ref().unwrap().view(), p, s).unwrap();
    p
}

pub fn ext_custom_sql_exec(env: &WasmosEnv, ptr: WasmPtr<u8>) -> WasmPtr<u8> {
    let req_str = str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize);
    let request = req_str.as_str();

    let res = tokio::task::block_in_place(move || {
        tokio::runtime::Handle::current().block_on(async move {
            let pool = env.db_pool.to_owned().read().await.to_owned().unwrap();
            let conn = pool.conn().await.unwrap();
            conn.exec(request).await.map_err(|e| e.to_string())
        })
    });
    let s = res
        .map(|r| {
            serde_json::to_string(&serde_json::json!({
                "ok": true,
                "data": r
            }))
            .unwrap()
        })
        .unwrap_or_else(|e| {
            serde_json::to_string(&serde_json::json!({
                "ok": false,
                "msg": e
            }))
            .unwrap()
        });

    let p = env
        .str_malloc
        .get_ref()
        .unwrap()
        .call(s.len() as _)
        .map_err(|e| dbg!(e))
        .unwrap();
    str_mem_write(&env.memory.get_ref().unwrap().view(), p, s).unwrap();
    p
}

pub fn call_wasm(
    exports: Exports,
    memory_view: MemoryView<u8>,
    f: String,
    args: Value,
) -> Result<Value, Box<dyn Error>> {
    let args = serde_json::to_string(&json!({ "body": args })).map_err(|e| dbg!(e))?;

    let f: NativeFunc<WasmPtr<u8>, WasmPtr<u8>> = exports
        .get_native_function(f.as_str())
        .map_err(|e| dbg!(e))?;

    let str_malloc: NativeFunc<u64, WasmPtr<u8>> = exports
        .get_native_function("str_malloc")
        .map_err(|e| dbg!(e))?;
    let args_p = str_malloc.call(args.len() as _).map_err(|e| dbg!(e))?;

    str_mem_write(&memory_view, args_p, args)?;

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
    String::from_utf8_lossy(data.as_slice()).to_string()
}

pub fn str_mem_write(
    memory_view: &MemoryView<u8>,
    ptr: WasmPtr<u8>,
    mut str: String,
) -> Result<(), Box<dyn Error>> {
    str.push('\0');

    for (i, c) in str.into_bytes().iter().enumerate() {
        memory_view.index(ptr.offset() as usize + i).replace(*c);
    }

    Ok(())
}
