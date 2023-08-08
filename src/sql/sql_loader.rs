use serde::Deserialize;
use std::error::Error;
use wasmer::{Extern, Instance, MemoryView};
use wasmos_types::sql::TableDDL;

use crate::sql::driver::{
    databend::{Databend, DatabendConnParams},
    model::DB,
};

use super::driver::{
    databend::DatabendPool,
    migration::migrate_table,
    model::{Conn, Pool},
};

#[derive(Debug)]
pub struct SqlModule {
    tables: Vec<TableDDL>,
    pub pool: Option<DatabendPool>,
}

impl SqlModule {
    pub async fn migrate(&self) -> Result<(), Box<dyn Error>> {
        let pool = self.pool.clone().ok_or("migration: no database pool")?;
        let conn = pool.conn().await?;

        let _ = conn
            .exec(format!("CREATE DATABASE IF NOT EXISTS {};", pool.db_name))
            .await;

        for t in &self.tables {
            migrate_table(t, pool.clone()).await?;
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Sql {
    pub modules: Vec<SqlModule>,
}

impl Sql {
    pub fn new() -> Self {
        Self { modules: vec![] }
    }

    pub async fn load_db_conn(instance: Instance) -> Result<DatabendPool, Box<dyn Error>> {
        let ptr = instance
            .exports
            .get_function("wasmos_settings_db_conn")?
            .call(&[])?;

        let memory = instance.exports.get_memory("memory")?;

        let memory_view: MemoryView<u8> = memory.view();
        let mut data: Vec<u8> = vec![];
        for v in memory_view[(ptr[0].unwrap_i32() as _)..].iter() {
            let v = v.get();
            if v == b'\0' {
                break;
            }
            data.push(v);
        }

        let res = String::from_utf8_lossy(data.as_slice());

        #[derive(Deserialize)]
        struct DbConn {
            url: String,
            db_name: String,
        }

        let res = serde_json::from_str::<DbConn>(&res)?;

        Ok(Databend::init(DatabendConnParams::new(
            res.url,
            res.db_name,
        ))?)
    }

    pub async fn load_ddl(instance: Instance) -> Result<SqlModule, Box<dyn Error>> {
        let handlers_metadata = instance
            .exports
            .iter()
            .filter(|e| e.0.starts_with("wasmos_table_ddl_"))
            .collect::<Vec<(&String, &Extern)>>();

        let memory = instance.exports.get_memory("memory")?;

        let tables = handlers_metadata
            .into_iter()
            .filter_map(|handler_metadata| {
                if let Extern::Function(metadata_f) = handler_metadata.1 {
                    (|| -> Result<TableDDL, Box<dyn Error>> {
                        let ptr = metadata_f.call(&[])?;

                        let memory_view: MemoryView<u8> = memory.view();
                        let mut data: Vec<u8> = vec![];
                        for v in memory_view[(ptr[0].unwrap_i32() as _)..].iter() {
                            let v = v.get();
                            if v == b'\0' {
                                break;
                            }
                            data.push(v);
                        }

                        let res = String::from_utf8_lossy(data.as_slice());

                        serde_json::from_str::<TableDDL>(&res).map_err(|e| e.into())
                    })()
                    .ok()
                } else {
                    None
                }
            })
            .collect();

        Ok(SqlModule {
            tables: tables,
            pool: Self::load_db_conn(instance).await.ok(),
        })
    }

    pub async fn migrate(self) -> Result<(), Box<dyn Error>> {
        for m in self.modules {
            m.migrate().await?
        }
        Ok(())
    }
}
