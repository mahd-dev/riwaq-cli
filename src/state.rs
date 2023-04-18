use std::{collections::HashMap, sync::Arc};

use async_graphql::{dynamic::Schema, EmptyMutation, EmptySubscription};
use tokio::sync::RwLock;

use crate::QueryRoot;

pub type StateOrgs = Arc<RwLock<HashMap<String, Org>>>;

#[derive(Debug, Default, Clone)]
pub struct Orgs {
    pub orgs: StateOrgs
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
