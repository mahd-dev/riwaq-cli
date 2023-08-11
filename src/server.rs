use std::{collections::HashMap, error::Error, sync::Arc};

use async_graphql::{
    futures_util::TryStreamExt, Context, EmptyMutation, EmptySubscription, Object, Schema,
};
use opendal::{EntryMode, Metakey, Operator};
use poem::{get, post, Route};
use tokio::sync::RwLock;

use crate::{
    api::{graphql_playground, GraphQL},
    state::{Orgs, State, StorageConfig},
};

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn load_wasm(&self, ctx: &Context<'_>, org: String) -> async_graphql::Result<bool> {
        let mut orgs = ctx.data::<Orgs>().unwrap().clone();
        orgs.load_wasm(org.as_str(), orgs.storage.clone())
            .await
            .map(|_| true)
            .map_err(|e| async_graphql::Error::new_with_source(e.to_string()))
    }
}

pub fn init_operator(storage: Arc<StorageConfig>) -> Result<Operator, Box<dyn Error>> {
    Ok(match storage.kind {
        opendal::Scheme::Azblob => {
            Operator::from_map::<opendal::services::Azblob>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Azdfs => {
            Operator::from_map::<opendal::services::Azdfs>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Fs => {
            Operator::from_map::<opendal::services::Fs>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Gcs => {
            Operator::from_map::<opendal::services::Gcs>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Ghac => {
            Operator::from_map::<opendal::services::Ghac>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Http => {
            Operator::from_map::<opendal::services::Http>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Ipmfs => {
            Operator::from_map::<opendal::services::Ipmfs>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Memory => {
            Operator::from_map::<opendal::services::Memory>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Obs => {
            Operator::from_map::<opendal::services::Obs>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Oss => {
            Operator::from_map::<opendal::services::Oss>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::S3 => {
            Operator::from_map::<opendal::services::S3>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Webdav => {
            Operator::from_map::<opendal::services::Webdav>(storage.opt.clone())?.finish()
        }
        opendal::Scheme::Webhdfs => {
            Operator::from_map::<opendal::services::Webhdfs>(storage.opt.clone())?.finish()
        }
        s => return Err(format!("unsupported storage type: '{s}'").into()),
    })
}

pub async fn init_server(storage: Arc<StorageConfig>) -> Result<(Route, Orgs), Box<dyn Error>> {
    let op = Arc::new(init_operator(storage.clone())?);

    let orgs = Orgs {
        orgs: Arc::new(RwLock::new(HashMap::new())),
        storage: storage.clone(),
    };

    let state = State {
        orgs: orgs.clone(),
        root: Some(
            Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
                .data(orgs.clone())
                .finish(),
        ),
    };

    let mut ds = op.list("/").await?;
    while let Some(de) = ds.try_next().await? {
        let meta = op.clone().metadata(&de, Metakey::Mode).await?;
        if let EntryMode::DIR = meta.mode() {
            let mut orgs = state.orgs.clone();
            let org = de.name().replace('/', "");
            let _ = orgs.load_wasm(org, storage.clone()).await;
        };
    }

    let app = Route::new()
        .at("/playground*path", get(graphql_playground))
        .at("/api*path", post(GraphQL { state }));

    Ok((app, orgs))
}
