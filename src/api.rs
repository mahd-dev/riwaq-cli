use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    Executor,
};
use async_graphql_poem::{GraphQLBatchRequest, GraphQLBatchResponse};
use poem::{
    async_trait, handler, http::StatusCode, web::Html, Endpoint, FromRequest, IntoResponse,
    Request, Response, Result,
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
        let uri = req.uri().to_string();
        let uri = uri.split('/').collect::<Vec<&str>>();
        let org = *uri.get(2).unwrap_or(&"");
        let req = GraphQLBatchRequest::from_request(&req, &mut body).await?;
        let schema;
        {
            schema = self
                .state
                .orgs
                .orgs
                .read()
                .await
                .get(org)
                .map(|s| s.gql.clone())
        }
        match schema {
            Some(gql) => Ok(GraphQLBatchResponse(gql.execute_batch(req.0).await)),
            None => match &self.state.root {
                Some(schema) if org.is_empty() => {
                    Ok(GraphQLBatchResponse(schema.execute_batch(req.0).await))
                }
                _ => Err(poem::Error::from(StatusCode::NOT_FOUND)),
            },
        }
    }
}

#[handler]
pub async fn graphql_playground(req: &Request) -> poem::Result<Response> {
    let uri = req.uri().to_string();
    let uri = uri.split('/').collect::<Vec<&str>>();
    let org = *uri.get(2).unwrap_or(&"");
    match org {
        "" => Ok(Html(playground_source(GraphQLPlaygroundConfig::new("/api/"))).into_response()),
        org => Ok(Html(playground_source(GraphQLPlaygroundConfig::new(
            format!("/api/{}", org).as_str(),
        )))
        .into_response()),
    }
}
