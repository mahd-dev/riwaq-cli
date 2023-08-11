mod api;
mod gql;
mod server;
mod sql;
mod state;
mod wasm;

use std::{collections::HashMap, process::Child, str::FromStr, sync::Arc, time::Duration};

use clap::Parser;
use poem::{listener::TcpListener, Server};
use std::process::Command;

use notify_debouncer_full::{
    new_debouncer,
    notify::{
        event::{AccessKind, AccessMode},
        EventKind, RecursiveMode, Watcher,
    },
};

use server::init_server;
use state::{StorageConfig, StorageOrgBy};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Parser)]
#[command(name = "riwaq")]
#[command(bin_name = "riwaq")]
enum RiwaqCli {
    Dev,
    Server,
}

fn build_project() -> Child {
    let toml = std::fs::read_to_string("./Cargo.toml")
        .unwrap()
        .parse::<toml::Table>()
        .unwrap();
    let pname = toml["package"]["name"].as_str().unwrap_or("riwaq");
    Command::new("sh")
    .arg("-c")
        .arg(format!("cargo build --target wasm32-unknown-unknown && mkdir -p ./dist/{pname} && mv target/wasm32-unknown-unknown/debug/{pname}.wasm ./dist/{pname}/{pname}.wasm &> /dev/null"))
        .spawn()
        .expect("Failed to build project")
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv::dotenv();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args = RiwaqCli::parse();

    let addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:50051".to_string());
    match args {
        RiwaqCli::Dev => {
            let storage = Arc::new(StorageConfig {
                kind: opendal::Scheme::from_str("fs").unwrap(),
                opt: HashMap::from([(
                    "root".to_string(),
                    {
                        let mut p = std::env::current_dir().unwrap();
                        p.push("dist");
                        p
                    }
                    .to_str()
                    .unwrap()
                    .to_string(),
                )]),
                org_by: StorageOrgBy::Dir,
            });

            let (route, orgs) = init_server(storage.clone()).await?;

            let build_handle = tokio::spawn(async {
                let (tx, rx) = std::sync::mpsc::channel();
                let mut watcher = new_debouncer(Duration::from_millis(250), None, tx).unwrap();
                watcher
                    .watcher()
                    .watch(
                        &{
                            let mut p = std::env::current_dir().unwrap();
                            p.push("src");
                            p
                        },
                        RecursiveMode::Recursive,
                    )
                    .unwrap();

                let mut c: Child = build_project();
                for res in rx {
                    if let Ok(_) = res {
                        let _ = c.kill();
                        c = build_project();
                    }
                }
            });

            let load_handle = tokio::spawn(async move {
                let (tx, rx) = std::sync::mpsc::channel();
                let mut watcher = new_debouncer(Duration::from_millis(250), None, tx).unwrap();
                watcher
                    .watcher()
                    .watch(
                        &{
                            let mut p = std::env::current_dir().unwrap();
                            p.push("dist");
                            p
                        },
                        RecursiveMode::Recursive,
                    )
                    .unwrap();

                for res in rx {
                    if let Ok(events) = res {
                        for e in events {
                            match e.kind {
                                EventKind::Access(AccessKind::Close(AccessMode::Write))
                                | EventKind::Modify(_)
                                | EventKind::Remove(_) => {
                                    for p in e.paths.to_owned().iter().filter(|p| p.is_file()) {
                                        let org = p
                                            .parent()
                                            .and_then(|t| t.file_name().and_then(|f| f.to_str()))
                                            .unwrap();
                                        let _ = orgs.clone().load_wasm(org, storage.clone()).await;
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }
            });

            let server_handle = tokio::spawn(async {
                let _ = Server::new(TcpListener::bind(addr))
                    .run(route)
                    .await
                    .map_err(|e| dbg!(e));
            });
            let _ = server_handle.await;
            let _ = load_handle.await;
            let _ = build_handle.await;
        }
        RiwaqCli::Server => {
            let (route, _) = init_server(Arc::new(StorageConfig {
                kind: opendal::Scheme::from_str(
                    &std::env::var("STORAGE_SCHEME").unwrap_or("fs".to_string()),
                )
                .unwrap(),
                opt: HashMap::from_iter(
                    std::env::vars()
                        .filter_map(|(k, v)| {
                            if k.starts_with("STORAGE_") {
                                Some((k.trim_start_matches("STORAGE_").to_string(), v))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<(String, String)>>(),
                ),
                org_by: StorageOrgBy::Bucket,
            }))
            .await?;
            let _ = Server::new(TcpListener::bind(addr))
                .run(route)
                .await
                .map_err(|e| dbg!(e));
        }
    };

    Ok(())
}
