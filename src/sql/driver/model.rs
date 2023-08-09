use std::error::Error;

use async_graphql::futures_util::future::BoxFuture;
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct SQLFilter(riwaq::sql::FilterItem);
impl riwaq::sql::SQLFilterTrait for SQLFilter {
    fn get_filter(&self) -> riwaq::sql::FilterItem {
        self.0.clone()
    }
}

pub trait Conn {
    fn exec<R>(&self, request: R) -> BoxFuture<Result<i64, Box<dyn Error>>>
    where
        R: ToString + Send;
    fn all(
        &self,
        request: riwaq::sql::Select<SQLFilter>,
    ) -> BoxFuture<Result<Vec<serde_json::Value>, Box<dyn Error>>>;

    fn custom_query(
        &self,
        request: String,
    ) -> BoxFuture<Result<Vec<Vec<serde_json::Value>>, Box<dyn Error>>>;
}

pub trait ConnParams {}

pub trait Pool {
    type ConnType: Conn;
    fn conn(&self) -> BoxFuture<Result<Self::ConnType, Box<dyn Error>>>;
    fn disconnect(&self) -> BoxFuture<Result<(), Box<dyn Error>>>;
}

pub trait DB {
    type ParamsType: ConnParams;
    type PoolType: Pool;

    fn init(params: Self::ParamsType) -> Result<Self::PoolType, Box<dyn Error>>;
}
