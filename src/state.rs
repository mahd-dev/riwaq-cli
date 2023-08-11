use std::{collections::HashMap, sync::Arc};

use async_graphql::{dynamic::Schema, EmptyMutation, EmptySubscription};
use tokio::sync::RwLock;

use crate::server::QueryRoot;

pub type StateOrgs = Arc<RwLock<HashMap<String, Org>>>;

#[derive(Debug, Clone)]
pub enum StorageOrgBy {
    Bucket,
    Dir,
}
impl Default for StorageOrgBy {
    fn default() -> Self {
        Self::Dir
    }
}

#[derive(Debug, Default, Clone)]
pub struct StorageConfig {
    pub kind: opendal::Scheme,
    pub opt: HashMap<String, String>,
    pub org_by: StorageOrgBy,
}

#[derive(Debug, Default, Clone)]
pub struct Orgs {
    pub orgs: StateOrgs,
    pub storage: Arc<StorageConfig>,
}

#[derive(Debug)]
pub struct Org {
    pub gql: Schema,
}

#[derive(Default)]
pub struct State {
    pub orgs: Orgs,
    pub root: Option<async_graphql::Schema<QueryRoot, EmptyMutation, EmptySubscription>>,
}
