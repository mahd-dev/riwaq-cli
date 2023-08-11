use std::{env, error::Error, sync::Arc};

use async_graphql::futures_util::TryStreamExt;
use tokio::sync::RwLock;
use wasmer::{
    imports, ChainableNamedResolver, Cranelift, Function, ImportObject, Instance, LazyInit, Memory,
    Module, NativeFunc, Singlepass, Store, Universal, UniversalEngine, WasmPtr,
};
use wasmer_wasi::{generate_import_object_from_env, WasiEnv, WasiState};

use crate::{
    gql::gql_loader::Gql,
    server::init_operator,
    sql::{
        driver::databend::DatabendPool,
        sql_loader::{Sql, SqlModule},
    },
    state::{Org, Orgs, StorageConfig, StorageOrgBy},
    wasm::wasm_helper::str_mem_read,
};

use super::wasm_helper::{ext_custom_sql_exec, ext_custom_sql_query, ext_sql_exec, ext_sql_query};

#[derive(Clone, wasmer::WasmerEnv)]
pub struct RiwaqEnv {
    #[wasmer(export)]
    pub memory: LazyInit<Memory>,
    #[wasmer(export)]
    pub str_malloc: LazyInit<NativeFunc<u64, WasmPtr<u8>>>,
    pub db_pool: Arc<RwLock<Option<DatabendPool>>>,
}

impl Orgs {
    pub async fn load_wasm<S>(
        &mut self,
        org: S,
        storage: Arc<StorageConfig>,
    ) -> Result<(), Box<dyn Error>>
    where
        S: Into<String> + Clone,
    {
        let mut gql = Gql::new();
        let mut sql = Sql::new();

        let compiler: UniversalEngine = match env::var("WASM_COMPILER")
            .unwrap_or("cranelift".to_string())
            .as_str()
        {
            "singlepass" => Universal::new(Singlepass::new()).engine(),
            // "llvm" => Universal::new(LLVM::new()).engine(),
            _ => Universal::new(Cranelift::new()).engine(),
        };
        let store = Store::new(&compiler);

        let mut storage = (*storage).clone();
        match storage.org_by {
            StorageOrgBy::Dir => storage
                .opt
                .get_mut("root")
                .map(|v| *v = format!("{v}/{org}", org = org.clone().into())),
            StorageOrgBy::Bucket => storage
                .opt
                .get_mut("bucket")
                .map(|v| *v = org.clone().into()),
        };

        let op = init_operator(Arc::new(storage.clone()))?;

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

            let wasi_env = WasiEnv::new(WasiState::new("riwaq").build()?);
            let objects = objects.chain_front(generate_import_object_from_env(
                &store,
                wasi_env.clone(),
                wasmer_wasi::WasiVersion::Snapshot1,
            ));

            let mut riwaq_env = RiwaqEnv {
                memory: LazyInit::new(),
                str_malloc: LazyInit::new(),
                db_pool: Arc::new(RwLock::new(None)),
            };

            let objects = objects.chain_front(imports! {
                "env" => {
                    "riwaq_dbg" => Function::new_native_with_env(&store, riwaq_env.clone(), |env: &RiwaqEnv, ptr: WasmPtr<u8>| {
                        println!(
                            "{}",
                            str_mem_read(&env.memory.get_ref().unwrap().view(), ptr.offset() as usize)
                        );
                    }),
                    "ext_sql_exec" => Function::new_native_with_env(&store, riwaq_env.clone(), ext_sql_exec),
                    "ext_sql_query" => Function::new_native_with_env(&store, riwaq_env.clone(), ext_sql_query),
                    "ext_custom_sql_exec" => Function::new_native_with_env(&store, riwaq_env.clone(), ext_custom_sql_exec),
                    "ext_custom_sql_query" => Function::new_native_with_env(&store, riwaq_env.clone(), ext_custom_sql_query)
                }
            });

            let instance = Instance::new(&module, &objects).map_err(|e| dbg!(e))?;

            riwaq_env
                .memory
                .initialize(instance.exports.get_memory("memory")?.to_owned());
            riwaq_env.str_malloc.initialize(
                instance
                    .exports
                    .get_native_function("str_malloc")?
                    .to_owned(),
            );
            let sql_module = Sql::load_ddl(instance.clone(), org.clone().into())
                .await
                .ok();
            if let Some(SqlModule {
                pool: Some(sql_pool),
                ..
            }) = &sql_module
            {
                let mut a = riwaq_env.db_pool.write().await;
                *a = Some(sql_pool.clone());
            };
            if let Some(qm) = sql_module {
                sql.modules.push(qm);
            };

            gql = gql.load_handlers(instance.clone())?;
        }

        let o: (String, Org) = (
            org.clone().into(),
            Org {
                gql: gql.build_schema().map_err(|e| dbg!(e))?,
            },
        );

        sql.migrate(org.clone()).await?;

        // let ex_pool;
        // {
        //     ex_pool = self
        //         .orgs
        //         .read()
        //         .await
        //         .get(&o.0)
        //         .map_or(None, |org| org.sql_pool.as_ref().map(|p| p.clone()));
        // }
        {
            self.orgs.write().await.insert(o.0, o.1);
        }
        // if let Some(p) = ex_pool {
        //     let _ = p.disconnect().await;
        // };
        Ok(())
    }
}
