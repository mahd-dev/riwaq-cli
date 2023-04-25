use std::{env, error::Error};

use async_graphql::futures_util::TryStreamExt;
use opendal::{layers::LoggingLayer, Builder, Operator};

use wasmer::{
    imports, ChainableNamedResolver, Cranelift, Function, ImportObject, Instance, LazyInit, Memory,
    Module, NativeFunc, Singlepass, Store, Universal, UniversalEngine, WasmPtr,
};
use wasmer_wasi::{generate_import_object_from_env, WasiEnv, WasiState};

use crate::{
    gql::gql_loader::Gql,
    state::{Org, Orgs},
    wasm::wasm_helper::{str_mem_read, str_mem_write},
};

impl Orgs {
    pub async fn load_wasm<B>(
        &mut self,
        org: impl Into<String>,
        builder: B,
    ) -> Result<(), Box<dyn Error>>
    where
        B: Builder,
    {
        let mut gql = Gql::new();

        let compiler: UniversalEngine = match env::var("WASM_COMPILER")
            .unwrap_or("cranelift".to_string())
            .as_str()
        {
            "singlepass" => Universal::new(Singlepass::new()).engine(),
            // "llvm" => Universal::new(LLVM::new()).engine(),
            _ => Universal::new(Cranelift::new()).engine(),
        };
        let store = Store::new(&compiler);

        let op = &Operator::new(builder)
            .unwrap()
            .layer(LoggingLayer::default())
            .finish();

        let mut modules = op
            .scan("/")
            .await
            .map_err(|e| e.with_context("op", "error listing files"))
            .map_err(|e| dbg!(e))?;

        while let Some(e) = modules
            .try_next()
            .await
            .map_err(|e| e.with_context("op", "error getting next file"))
            .map_err(|e| dbg!(e))?
        {
            if e.name().starts_with(',') || !e.name().ends_with(".wasm") {
                continue;
            }
            let path = e.path();
            let res = op
                .read(path)
                .await
                .map_err(|e| e.with_context("op", "error reading file"))
                .map_err(|e| dbg!(e))?;

            let module = Module::new(&store, res).map_err(|e| dbg!(e))?;

            let objects = ImportObject::new();

            let wasi_env = WasiEnv::new(WasiState::new("wasmos").build()?);
            let objects = objects.chain_front(generate_import_object_from_env(
                &store,
                wasi_env.clone(),
                wasmer_wasi::WasiVersion::Snapshot1,
            ));

            #[derive(Clone, wasmer::WasmerEnv)]
            pub struct WasmosEnv {
                #[wasmer(export)]
                memory: LazyInit<Memory>,
                #[wasmer(export)]
                str_malloc: LazyInit<NativeFunc<u64, WasmPtr<u8>>>,
            }

            let mut wasmos_env = WasmosEnv {
                memory: LazyInit::new(),
                str_malloc: LazyInit::new(),
            };

            let objects = objects.chain_front(imports! {
                "env" => {
                    "wasmos_dbg" => Function::new_native_with_env(&store, wasmos_env.clone(), |env: &WasmosEnv, ptr: WasmPtr<u8>| {
                        println!(
                            "{}",
                            str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize)
                        );
                    }),
                    "sql_dml" => Function::new_native_with_env(&store, wasmos_env.clone(), |env: &WasmosEnv, ptr: WasmPtr<u8>| -> WasmPtr<u8> {
                        let mut s = str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize);
                        s.push('\0');
                        let p = env.str_malloc.get_ref().unwrap().call(s.len() as _).map_err(|e| dbg!(e)).unwrap();
                        str_mem_write(&env.memory.get_ref().unwrap().view(), p, s).unwrap();
                        p
                    })
                }
            });

            let instance = Instance::new(&module, &objects).map_err(|e| dbg!(e))?;

            wasmos_env
                .memory
                .initialize(instance.exports.get_memory("memory")?.to_owned());
            wasmos_env.str_malloc.initialize(
                instance
                    .exports
                    .get_native_function("str_malloc")?
                    .to_owned(),
            );

            gql = gql.load_handlers(instance.clone())?;
        }

        let o: (String, Org) = (
            org.into(),
            Org {
                gql: gql.build_schema().map_err(|e| dbg!(e))?,
            },
        );
        {
            self.orgs.write().await.insert(o.0, o.1);
        }
        Ok(())
    }
}
