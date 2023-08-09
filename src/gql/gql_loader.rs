use std::error::Error;

use async_graphql::dynamic::{Field, FieldFuture, InputObject, Object, Schema, TypeRef};
use serde::Deserialize;
use serde_json::json;
use wasmer::{Extern, Instance, MemoryView};

use crate::{
    gql::gql_helper::{ser_params, value_to_gql_input_type, value_to_gql_output_type},
    wasm::wasm_helper::call_wasm,
};

pub struct Gql {
    input_objects: Vec<InputObject>,
    output_objects: Vec<Object>,
    query: Object,
    // mutation: Object,
    // subscription: Subscription,
    contain_fields: bool,
}

impl Gql {
    pub fn new() -> Self {
        Self {
            input_objects: vec![],
            output_objects: vec![],
            query: Object::new("Query"),
            // mutation: Object::new("Mutation"),
            // subscription: Subscription::new("Subscription"),
            contain_fields: false,
        }
    }

    pub fn load_handlers(mut self, instance: Instance) -> Result<Self, Box<dyn Error>> {
        let handlers_metadata = instance
            .exports
            .iter()
            .filter(|e| e.0.starts_with("riwaq_handler_metadata_"))
            .collect::<Vec<(&String, &Extern)>>();

        let memory = instance.exports.get_memory("memory")?;

        for handler_metadata in handlers_metadata {
            if let Extern::Function(metadata_f) = handler_metadata.1 {
                let ptr = metadata_f.call(&[]).unwrap();

                let memory_view: MemoryView<u8> = memory.view();
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
                    .strip_prefix("riwaq_handler_metadata_")
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
                                let e2 = instance.exports;
                                let memory = e.get_memory("memory")?;
                                let memory_view: MemoryView<u8> = memory.view();
                                let res = call_wasm(
                                    e2,
                                    memory_view,
                                    format!("riwaq_handler_{}", f_name.clone()),
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
                self.query = self.query.field(field);
                self.input_objects.extend(input_fields.2);
                self.output_objects.extend(output_fields.2);

                self.contain_fields = true;
            }
        }
        Ok(self)
    }

    pub fn build_schema(self) -> Result<Schema, Box<dyn Error>> {
        if !self.contain_fields {
            return Err("wasm does not contain any object".into());
        }

        let mut schema = Schema::build(self.query.type_name(), None, None).register(self.query);
        for io in self.input_objects {
            schema = schema.register(io);
        }
        for o in self.output_objects {
            schema = schema.register(o);
        }

        schema.finish().map_err(|e| e.into())
    }
}
