mod api;
mod gql;
mod sql;
mod state;
mod wasm;

use std::{collections::HashMap, env, error::Error, sync::Arc};

use async_graphql::{
    futures_util::TryStreamExt, Context, EmptyMutation, EmptySubscription, Object, Schema,
};
use opendal::{EntryMode, Metakey, Operator};
use poem::{get, listener::TcpListener, post, Route, Server};
use tokio::sync::RwLock;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{
    api::{graphql_playground, GraphQL},
    state::{Orgs, State},
};

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn load_wasm(&self, ctx: &Context<'_>, org: String) -> async_graphql::Result<bool> {
        let mut orgs = ctx.data::<Orgs>().unwrap().clone();
        let mut builder = opendal::services::Fs::default();
        builder.root(format!("wasm/{}", org.clone()).as_str());
        orgs.load_wasm(org.as_str(), builder)
            .await
            .map(|_| true)
            .map_err(|e| dbg!(e))
            .map_err(|e| async_graphql::Error::new_with_source(e.to_string()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv()?;

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let orgs = Orgs {
        orgs: Arc::new(RwLock::new(HashMap::new())),
    };

    let state = State {
        orgs: orgs.clone(),
        root: Some(
            Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
                .data(orgs.clone())
                .finish(),
        ),
    };

    let mut builder = opendal::services::Fs::default();
    builder.root("wasm");
    let op = Operator::new(builder)?.finish();

    let mut ds = op.list("/").await?;
    while let Some(de) = ds.try_next().await? {
        let meta = op.metadata(&de, Metakey::Mode).await?;
        if let EntryMode::DIR = meta.mode() {
            let mut orgs = state.orgs.clone();
            let mut builder = opendal::services::Fs::default();
            let org = de.name().replace('/', "");

            builder.root(format!("wasm/{}", org).as_str());
            let _ = orgs.load_wasm(org, builder).await;
        };
    }

    let addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    let app = Route::new()
        .at("/playground*path", get(graphql_playground))
        .at("/api*path", post(GraphQL { state }));

    Server::new(TcpListener::bind(addr)).run(app).await?;
    Ok(())
}
