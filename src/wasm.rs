use std::{env, error::Error};

use async_graphql::{
    dynamic::{Field, FieldFuture, InputObject, Object, Schema, TypeRef},
    futures_util::TryStreamExt,
};
use opendal::{layers::LoggingLayer, Builder, Operator};

use serde::Deserialize;
use serde_json::json;
use wasmer::{
    ChainableNamedResolver, Cranelift, Extern, ImportObject, Instance, MemoryView, Module,
    Singlepass, Store, Universal, UniversalEngine,
};
use wasmer_wasi::{generate_import_object_from_env, WasiEnv, WasiState};

use crate::{
    helper::{call_wasm, ser_params, value_to_gql_input_type, value_to_gql_output_type},
    state::{Org, State},
};

impl State {
    pub async fn load_wasm<B>(
        &mut self,
        org: impl Into<String>,
        builder: B,
    ) -> Result<(), Box<dyn Error>>
    where
        B: Builder,
    {
        let mut input_objects: Vec<InputObject> = vec![];
        let mut output_objects: Vec<Object> = vec![];
        let mut query = Object::new("Query");
        // let mutation = Object::new("Mutation");
        // let subscription = Subscription::new("Subscription");

        let compiler: UniversalEngine = match env::var("WASM_COMPILER")
            .unwrap_or("cranelift".to_string())
            .as_str()
        {
            "singlepass" => Universal::new(Singlepass::new()).engine(),
            // "llvm" => Universal::new(LLVM::new()).engine(),
            _ => Universal::new(Cranelift::new()).engine(),
        };
        let mut store = Store::new(&compiler);

        let op = &Operator::new(builder)
            .unwrap()
            .layer(LoggingLayer::default())
            .finish();

        let mut modules = op
            .list("/")
            .await
            .map_err(|e| e.with_context("op", "error listing files"))
            .map_err(|e| dbg!(e))?;

        while let Some(e) = modules
            .try_next()
            .await
            .map_err(|e| e.with_context("op", "error getting next file"))
            .map_err(|e| dbg!(e))?
        {
            let path = e.path().clone();
            let res = op
                .read(path)
                .await
                .map_err(|e| e.with_context("op", "error reading file"))
                .map_err(|e| dbg!(e))?;

            let module = Module::new(&store, res).map_err(|e| dbg!(e))?;

            let objects = ImportObject::new();

            let wasi_env = WasiEnv::new(WasiState::new("wasmos").build()?.into());
            let objects = objects.chain_front(generate_import_object_from_env(
                &mut store,
                wasi_env,
                wasmer_wasi::WasiVersion::Snapshot1,
            ));

            let instance = Instance::new(&module, &objects).map_err(|e| dbg!(e))?;
            let handlers_metadata = instance
                .exports
                .iter()
                .filter(|e| e.0.starts_with("wasmos_handler_metadata_"))
                .collect::<Vec<(&String, &Extern)>>();

            let memory = instance.exports.get_memory("memory")?;
            let memory_view: MemoryView<u8> = memory.view();

            for handler_metadata in handlers_metadata {
                if let Extern::Function(metadata_f) = handler_metadata.1 {
                    let ptr = metadata_f.call(&[]).unwrap();

                    let mut data: Vec<u8> = vec![];
                    for v in memory_view[(ptr[0].unwrap_i32() as _)..].iter() {
                        let v = v.get();
                        if v == b'\0' {
                            break;
                        }
                        data.push(v);
                    }

                    #[derive(Deserialize, Debug)]
                    struct Metadata {
                        input: serde_json::Value,
                        output: serde_json::Value,
                    }

                    let res = String::from_utf8_lossy(data.as_slice());

                    let metadata = serde_json::from_str::<Metadata>(&res).unwrap();

                    let input_fields =
                        value_to_gql_input_type("input".to_string(), metadata.input.clone())?;
                    let output_fields = value_to_gql_output_type(
                        "output".to_string(),
                        json!({
                            "container": "Obj",
                            "content": metadata.output.clone()
                        }),
                    )?;

                    let f_name = handler_metadata
                        .0
                        .strip_prefix("wasmos_handler_metadata_")
                        .ok_or("")?
                        .to_string();

                    let instance = instance.clone();

                    let mut field = Field::new(
                        f_name.clone(),
                        TypeRef::named_nn(output_fields.0),
                        move |ctx| {
                            let r: Result<Option<async_graphql::Value>, async_graphql::Error> =
                                (|| {
                                    let instance = instance.clone();
                                    let e = instance.exports.clone();
                                    let e2 = instance.exports.clone();
                                    let memory = e.get_memory("memory")?;
                                    let memory_view: MemoryView<u8> = memory.view();
                                    let res = call_wasm(
                                        e2,
                                        memory_view,
                                        format!("wasmos_handler_{}", f_name.clone()),
                                        ser_params(ctx),
                                    )
                                    .map_err(|e| {
                                        async_graphql::Error::new_with_source(e.to_string())
                                    })?;
                                    Ok(Some(async_graphql::Value::from_json(res)?))
                                })();
                            FieldFuture::new(async move { r })
                        },
                    );
                    for f in input_fields.1 {
                        field = field.argument(f);
                    }
                    query = query.field(field);
                    input_objects.extend(input_fields.2);
                    output_objects.extend(output_fields.2);
                }
            }
        }

        let mut schema = Schema::build(query.type_name(), None, None).register(query);
        for io in input_objects {
            schema = schema.register(io);
        }
        for o in output_objects {
            schema = schema.register(o);
        }

        self.orgs.insert(
            org.into(),
            Org {
                gql: schema.finish().map_err(|e| dbg!(e))?,
            },
        );

        Ok(())
    }
}

// impl Org {
//     pub async fn load_wasm_module(&mut self, module: Vec<u8>) {
//       let myobj = Object::new("MyObj")
//             .field(Field::new("a", TypeRef::named(TypeRef::INT), |_| {
//                 FieldFuture::new(async { Ok(Some(Value::from(123))) })
//             }))
//             .field(Field::new("b", TypeRef::named(TypeRef::STRING), |_| {
//                 FieldFuture::new(async { Ok(Some(Value::from("abc"))) })
//             }));

//         let query = Object::new("Query")
//             .field(Field::new("value", TypeRef::named(TypeRef::INT), |_| {
//                 FieldFuture::new(async { Ok(Some(Value::from(100))) })
//             }))
//             .field(Field::new(
//                 "valueObj",
//                 TypeRef::named_nn(myobj.type_name()),
//                 |_| FieldFuture::new(async { Ok(Some(FieldValue::NULL)) }),
//             ));
//         let schema = Schema::build("Query", None, None)
//             .register(query)
//             .register(myobj)
//             .finish()
//             .unwrap();

//         // self.gql.

//     }
// }
