use async_graphql::futures_util::StreamExt;
use databend_driver::Value;
use wasmos_types::sql::{TableDDL, TableDDLOp};

use super::{
    databend::DatabendPool,
    model::{Conn, Pool},
};
use std::error::Error;

pub async fn migrate_table(ddl: &TableDDL, pool: DatabendPool) -> Result<(), Box<dyn Error>> {
    let t_name = format!("{}.{}", pool.db_name, ddl.name);
    let conn = pool.conn().await.unwrap();

    if let TableDDLOp::Drop = ddl.op {
        let _ = dbg!(conn.exec(format!("DROP TABLE IF EXISTS {};", t_name)).await);
        return Ok(());
    } else if let TableDDLOp::DropAll = ddl.op {
        let _ = dbg!(
            conn.exec(format!("DROP TABLE IF EXISTS {} ALL;", t_name))
                .await
        );
        return Ok(());
    } else if let TableDDLOp::Undrop = ddl.op {
        let _ = dbg!(conn.exec(format!("UNDROP TABLE {};", t_name)).await);
    }

    let cols = ddl
        .cols
        .iter()
        .map(|col| {
            format!(
                "{} {}{}",
                col.name,
                col.ty,
                if col.opt { " NULL" } else { "" }
            )
        })
        .collect::<Vec<String>>();

    for col in ddl.cols.iter() {
        let _ = dbg!(
            conn.exec(format!(
                "ALTER TABLE IF EXISTS {} ADD COLUMN {} {} {};",
                t_name,
                col.name,
                col.ty,
                if col.opt { "NULL" } else { "NOT NULL" }
            ))
            .await
        );
    }

    let _ = dbg!(
        conn.exec(format!(
            "ALTER TABLE IF EXISTS {} MODIFY COLUMN {};",
            t_name,
            cols.join(", COLUMN ")
        ))
        .await
    );

    let tbl_exists = conn
        .conn
        .query_iter(format!("DESC {}", t_name).as_str())
        .await;

    if let Ok(mut tbl_des) = tbl_exists {
        while let Some(row) = tbl_des.next().await {
            let row = match row {
                Ok(r) => r,
                Err(_) => continue,
            };
            let col_name = row
                .values()
                .get(0)
                .and_then(|res| match res {
                    Value::String(v) => Some(v.to_owned()),
                    _ => None,
                })
                .unwrap_or_else(String::new);
            if let None = ddl.cols.iter().find(|r| r.name == col_name) {
                let _ = conn
                    .exec(format!("ALTER TABLE {} DROP COLUMN {};", t_name, col_name))
                    .await;
            };
        }
    }

    let _ = conn
        .exec(format!(
            "CREATE TABLE IF NOT EXISTS {} ({});",
            t_name,
            cols.join(", ")
        ))
        .await;

    Ok(())
}
