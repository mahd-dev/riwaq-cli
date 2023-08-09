use std::{error::Error, sync::Arc};

use async_graphql::futures_util::{future::BoxFuture, FutureExt, StreamExt};
use chrono::{NaiveDate, NaiveDateTime};

use super::model::{Conn, ConnParams, Pool, DB};

pub struct DatabendConn {
    pub conn: Arc<Box<dyn databend_driver::Connection>>,
}
impl Conn for DatabendConn {
    fn exec<R>(&self, request: R) -> BoxFuture<Result<i64, Box<dyn Error>>>
    where
        R: ToString + Send,
    {
        let req = request.to_string();
        async move { self.conn.exec(&req.to_string()).await.map_err(|e| e.into()) }.boxed()
    }

    fn all(
        &self,
        request: wasmos::sql::Select<super::model::SQLFilter>,
    ) -> BoxFuture<Result<Vec<serde_json::Value>, Box<dyn Error>>> {
        async move {
            let mut rows = self.conn.query_iter(&request.to_string()).await.unwrap();
            let mut res = vec![];
            while let Some(row) = rows.next().await {
                let row = match row {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let mut r = serde_json::Map::new();
                for (col, value) in request.cols.clone().into_iter().zip(row.values()) {
                    r.insert(col.to_string(), databend_to_serde(value.clone()));
                }
                res.push(serde_json::Value::Object(r));
            }
            Ok(res)
        }
        .boxed()
    }

    fn custom_query(
        &self,
        request: String,
    ) -> BoxFuture<Result<Vec<Vec<serde_json::Value>>, Box<dyn Error>>> {
        async move {
            let mut rows = self.conn.query_iter(&request).await.unwrap();
            let mut res = vec![];
            while let Some(row) = rows.next().await {
                let row = match row {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                res.push(
                    row.values()
                        .into_iter()
                        .map(|v| databend_to_serde(v.to_owned()))
                        .collect(),
                );
            }
            Ok(res)
        }
        .boxed()
    }
}

pub struct DatabendConnParams {
    conn_str: String,
    db_name: String,
}
impl DatabendConnParams {
    pub fn new(conn_str: String, db_name: String) -> Self {
        Self { conn_str, db_name }
    }
}

impl ConnParams for DatabendConnParams {}

#[derive(Clone, Debug)]
pub struct DatabendPool {
    pub conn_str: String,
    pub db_name: String,
}

impl Pool for DatabendPool {
    type ConnType = DatabendConn;

    fn conn(&self) -> BoxFuture<Result<DatabendConn, Box<dyn Error>>> {
        async {
            Ok(DatabendConn {
                conn: Arc::new(
                    databend_driver::Client::new(self.conn_str.to_owned())
                        .get_conn()
                        .await?,
                ),
            })
        }
        .boxed()
    }

    fn disconnect(&self) -> BoxFuture<Result<(), Box<dyn Error>>> {
        async { Ok(()) }.boxed()
    }
}

pub struct Databend {}

impl DB for Databend {
    type ParamsType = DatabendConnParams;
    type PoolType = DatabendPool;

    fn init(params: DatabendConnParams) -> Result<DatabendPool, Box<dyn Error>> {
        Ok(DatabendPool {
            conn_str: params.conn_str,
            db_name: params.db_name,
        })
    }
}

fn databend_to_serde(value: databend_driver::Value) -> serde_json::Value {
    match value {
        databend_driver::Value::Null => serde_json::Value::Null,
        databend_driver::Value::Boolean(v) => serde_json::to_value(v).unwrap(),
        databend_driver::Value::String(v) => serde_json::to_value(v).unwrap(),
        databend_driver::Value::Number(v) => match v {
            databend_driver::NumberValue::Int8(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Int16(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Int32(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Int64(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::UInt8(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::UInt16(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::UInt32(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::UInt64(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Float32(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Float64(n) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Decimal128(n, _) => serde_json::to_value(n).unwrap(),
            databend_driver::NumberValue::Decimal256(n, _) => {
                serde_json::to_value(n.as_i128()).unwrap()
            }
        },
        databend_driver::Value::Timestamp(_) => serde_json::to_value(
            NaiveDateTime::try_from(value)
                .unwrap()
                .format("%Y-%m-%dT%H:%M:%S%.f+00:00")
                .to_string(),
        )
        .unwrap(),
        databend_driver::Value::Date(_) => {
            serde_json::to_value(NaiveDate::try_from(value).unwrap().format("%F").to_string())
                .unwrap()
        }
    }
}
