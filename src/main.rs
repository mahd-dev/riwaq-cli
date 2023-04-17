mod api;
mod state;
mod wasm;
mod helper;

use std::{collections::HashMap, env, error::Error};

use poem::{get, listener::TcpListener, post, Route, Server};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::{
    api::{graphql_playground, GraphQL},
    state::State,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    dotenv::dotenv()?;

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let mut state = State {
        orgs: HashMap::new(),
    };

    let mut builder = opendal::services::Fs::default();
    builder.root("wasm/abc");
    let a = state.load_wasm("abc", builder).await;
    let _ = dbg!(a);

    let addr = env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    let app = Route::new()
        .at("/playground/:org", get(graphql_playground))
        .at("/api/:org", post(GraphQL { state }));

    Server::new(TcpListener::bind(addr)).run(app).await?;
    Ok(())
}
