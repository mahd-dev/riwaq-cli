use riwaq_types::sql::TableDDL;
use serde::Deserialize;
use std::error::Error;
use wasmer::{Extern, Instance, MemoryView};

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
    pub async fn migrate<S>(&self, org: S) -> Result<(), Box<dyn Error>>
    where
        S: Into<String> + Clone,
    {
        let pool = self.pool.clone().ok_or("migration: no database pool")?;
        let conn = pool.conn().await?;

        let _ = conn
            .exec(format!(
                "CREATE DATABASE IF NOT EXISTS {};",
                org.clone().into()
            ))
            .await;

        for t in &self.tables {
            migrate_table(t, pool.clone(), org.clone()).await?;
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

    pub async fn load_db_conn(
        instance: Instance,
        org: String,
    ) -> Result<DatabendPool, Box<dyn Error>> {
        #[derive(Deserialize)]
        struct DbConn {
            url: String,
        }

        let db_conn = match instance.exports.get_function("riwaq_settings_db_conn") {
            Ok(f) => {
                let ptr = f.call(&[])?;

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

                serde_json::from_str::<DbConn>(&res)?
            }
            Err(_) => DbConn {
                url: std::env::var("DB_URL")?
                    .to_string()
                    .replace("{{org}}", &org),
            },
        };

        Ok(Databend::init(DatabendConnParams::new(db_conn.url))?)
    }

    pub async fn load_ddl(instance: Instance, org: String) -> Result<SqlModule, Box<dyn Error>> {
        let handlers_metadata = instance
            .exports
            .iter()
            .filter(|e| e.0.starts_with("riwaq_table_ddl_"))
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
            pool: Self::load_db_conn(instance, org).await.ok(),
        })
    }

    pub async fn migrate<S>(self, org: S) -> Result<(), Box<dyn Error>>
    where
        S: Into<String> + Clone,
    {
        for m in self.modules {
            m.migrate(org.clone()).await?
        }
        Ok(())
    }
}
