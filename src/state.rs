use std::collections::HashMap;

use async_graphql::dynamic::Schema;

pub struct Org {
    pub gql: Schema,
}

#[derive(Default)]
pub struct State {
    pub orgs: HashMap<String, Org>,
}
