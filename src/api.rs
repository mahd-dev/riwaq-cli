use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    Executor,
};
use async_graphql_poem::{GraphQLBatchRequest, GraphQLBatchResponse};
use poem::{
    async_trait, handler, http::StatusCode, web::Html, Endpoint, FromRequest, IntoResponse,
    Request, Result,
};

use crate::state::State;

#[derive(Default)]
pub struct GraphQL {
    pub state: State,
}

#[async_trait]
impl Endpoint for GraphQL {
    type Output = GraphQLBatchResponse;

    async fn call(&self, req: Request) -> Result<Self::Output> {
        let (req, mut body) = req.split();
        let org = req.path_params::<String>()?;
        let req = GraphQLBatchRequest::from_request(&req, &mut body).await?;
        match self.state.orgs.get(&org) {
            Some(schema) => Ok(GraphQLBatchResponse(schema.gql.execute_batch(req.0).await)),
            None => Err(StatusCode::NOT_FOUND.into()),
        }
    }
}

#[handler]
pub async fn graphql_playground(req: &Request) -> impl IntoResponse {
    let org = req.path_params::<String>().unwrap();
    Html(playground_source(GraphQLPlaygroundConfig::new(
        format!("/api/{}", org).as_str(),
    )))
}
