use std::error::Error;

use async_graphql::dynamic::{
    Field, FieldFuture, FieldValue, InputObject, InputValue, ListAccessor, Object, ObjectAccessor,
    ResolverContext, TypeRef, ValueAccessor,
};
use serde_json::{Map, Value};

#[derive(Debug)]
pub enum TypeRefKind {
    Named,
    NamedNN,
    NamedList,
    NamedNNList,
    NamedListNN,
    NamedNNListNN,
}

pub type InputType = (String, Vec<InputValue>, Vec<InputObject>, TypeRefKind);

pub fn value_to_gql_input_type(
    name: String,
    metadata: serde_json::Value,
) -> Result<InputType, Box<dyn Error>> {
    match &metadata {
        serde_json::Value::String(t) => {
            let t = match t.as_str() {
                "bool" => TypeRef::BOOLEAN,
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
                | "u128" | "usize" => TypeRef::INT,
                "f32" | "f64" => TypeRef::FLOAT,
                "char" | "String" => TypeRef::STRING,
                _ => return Err(format!("invalid metadata type: {}", metadata).into()),
            };
            Ok((
                t.to_owned(),
                vec![InputValue::new(name, TypeRef::named_nn(t))],
                vec![],
                TypeRefKind::NamedNN,
            ))
        }
        serde_json::Value::Object(object) => {
            if let Some(Value::String(obj_name)) = object.get("_name_") {
                let fields = object
                    .iter()
                    .filter_map(|item| {
                        if item.0 != "_name_" {
                            Some(
                                value_to_gql_input_type(item.0.to_owned(), item.1.to_owned())
                                    .unwrap(),
                            )
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let mut inputs = vec![];
                let mut objects = vec![];
                for f in fields {
                    for v in f.1 {
                        inputs.push(v);
                    }
                    for v in f.2 {
                        objects.push(v);
                    }
                }
                Ok((obj_name.to_owned(), inputs, objects, TypeRefKind::NamedNN))
            } else if let Some(Value::String(container)) = object.get("container") {
                match container.as_str() {
                    "Vec" => {
                        let content = value_to_gql_input_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let t = match content.3 {
                            TypeRefKind::Named => (
                                TypeRef::named_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedListNN,
                            ),
                            TypeRefKind::NamedNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedList => (
                                TypeRef::named_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedListNN,
                            ),
                            TypeRefKind::NamedNNList => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedListNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedNNListNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                        };
                        Ok((
                            content.0.to_owned(),
                            vec![InputValue::new(name, t.0)],
                            content.2,
                            t.1,
                        ))
                    }
                    "Option" => {
                        let content = value_to_gql_input_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let t = match content.3 {
                            TypeRefKind::Named => {
                                (TypeRef::named(content.0.to_owned()), TypeRefKind::Named)
                            }
                            TypeRefKind::NamedNN => {
                                (TypeRef::named(content.0.to_owned()), TypeRefKind::Named)
                            }
                            TypeRefKind::NamedList => (
                                TypeRef::named_list(content.0.to_owned()),
                                TypeRefKind::NamedList,
                            ),
                            TypeRefKind::NamedNNList => (
                                TypeRef::named_nn_list(content.0.to_owned()),
                                TypeRefKind::NamedNNList,
                            ),
                            TypeRefKind::NamedListNN => (
                                TypeRef::named_list(content.0.to_owned()),
                                TypeRefKind::NamedList,
                            ),
                            TypeRefKind::NamedNNListNN => (
                                TypeRef::named_nn_list(content.0.to_owned()),
                                TypeRefKind::NamedNNList,
                            ),
                        };
                        Ok((
                            content.0.to_owned(),
                            vec![InputValue::new(name, t.0)],
                            content.2,
                            t.1,
                        ))
                    }
                    "Obj" => {
                        let content = value_to_gql_input_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let mut o = InputObject::new(content.0.to_owned());
                        for f in content.1 {
                            o = o.field(f);
                        }
                        let mut objects = vec![o];
                        for obj in content.2 {
                            objects.push(obj);
                        }
                        Ok((
                            content.0.to_owned(),
                            vec![InputValue::new(name, TypeRef::named_nn(content.0))],
                            objects,
                            TypeRefKind::NamedNN,
                        ))
                    }
                    _ => return Err(format!("invalid metadata type: {}", metadata).into()),
                }
            } else {
                Err("invalid metadata content".to_string().into())
            }
        }
        e => Err(format!("invalid metadata type: {}", e).into()),
    }
}

pub type OutputType = (String, Vec<Field>, Vec<Object>, TypeRefKind);

pub fn value_to_gql_output_type(
    name: String,
    metadata: serde_json::Value,
) -> Result<OutputType, Box<dyn Error>> {
    match &metadata {
        serde_json::Value::String(t) => {
            let t = match t.as_str() {
                "bool" => TypeRef::BOOLEAN,
                "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64"
                | "u128" | "usize" => TypeRef::INT,
                "f32" | "f64" => TypeRef::FLOAT,
                "char" | "String" => TypeRef::STRING,
                _ => return Err(format!("invalid metadata type: {}", metadata).into()),
            };
            Ok((
                t.to_owned(),
                vec![Field::new(name.clone(), TypeRef::named_nn(t), move |ctx| {
                    let v = (|| {
                        Ok(match ctx
                            .parent_value
                            .as_value()
                            .ok_or(async_graphql::Error::new(format!(
                                "could not parse parent value of {}",
                                name.to_owned()
                            )))? {
                            async_graphql::Value::Object(o) => o.get(name.as_str()),
                            _ => None,
                        }
                        .filter(|cv| async_graphql::Value::Null != **cv)
                        .map(|cv| FieldValue::value(cv.to_owned())))
                    })();
                    FieldFuture::new(async move { v })
                })],
                vec![],
                TypeRefKind::NamedNN,
            ))
        }
        serde_json::Value::Object(object) => {
            if let Some(Value::String(obj_name)) = object.get("_name_") {
                let fields = object
                    .iter()
                    .filter_map(|item| {
                        if item.0 != "_name_" {
                            Some(
                                value_to_gql_output_type(item.0.to_owned(), item.1.to_owned())
                                    .unwrap(),
                            )
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                let mut inputs = vec![];
                let mut objects = vec![];
                for f in fields {
                    for v in f.1 {
                        inputs.push(v);
                    }
                    for v in f.2 {
                        objects.push(v);
                    }
                }
                Ok((obj_name.to_owned(), inputs, objects, TypeRefKind::NamedNN))
            } else if let Some(Value::String(container)) = object.get("container") {
                match container.as_str() {
                    "Vec" => {
                        let content = value_to_gql_output_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let t = match content.3 {
                            TypeRefKind::Named => (
                                TypeRef::named_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedListNN,
                            ),
                            TypeRefKind::NamedNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedList => (
                                TypeRef::named_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedListNN,
                            ),
                            TypeRefKind::NamedNNList => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedListNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                            TypeRefKind::NamedNNListNN => (
                                TypeRef::named_nn_list_nn(content.0.to_owned()),
                                TypeRefKind::NamedNNListNN,
                            ),
                        };
                        Ok((
                            content.0.to_owned(),
                            vec![Field::new(name.to_owned(), t.0, move |ctx| {
                                let v = (|| {
                                    Ok(match ctx.parent_value.as_value().ok_or(
                                        async_graphql::Error::new(format!(
                                            "could not parse parent value of {}",
                                            name.to_owned()
                                        )),
                                    )? {
                                        async_graphql::Value::Object(o) => o.get(name.as_str()),
                                        _ => None,
                                    }
                                    .filter(|cv| async_graphql::Value::Null != **cv)
                                    .map(|cv| FieldValue::value(cv.to_owned())))
                                })();
                                FieldFuture::new(async move { v })
                            })],
                            content.2,
                            t.1,
                        ))
                    }
                    "Option" => {
                        let content = value_to_gql_output_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let t = match content.3 {
                            TypeRefKind::Named => {
                                (TypeRef::named(content.0.to_owned()), TypeRefKind::Named)
                            }
                            TypeRefKind::NamedNN => {
                                (TypeRef::named(content.0.to_owned()), TypeRefKind::Named)
                            }
                            TypeRefKind::NamedList => (
                                TypeRef::named_list(content.0.to_owned()),
                                TypeRefKind::NamedList,
                            ),
                            TypeRefKind::NamedNNList => (
                                TypeRef::named_nn_list(content.0.to_owned()),
                                TypeRefKind::NamedNNList,
                            ),
                            TypeRefKind::NamedListNN => (
                                TypeRef::named_list(content.0.to_owned()),
                                TypeRefKind::NamedList,
                            ),
                            TypeRefKind::NamedNNListNN => (
                                TypeRef::named_nn_list(content.0.to_owned()),
                                TypeRefKind::NamedNNList,
                            ),
                        };
                        Ok((
                            content.0.to_owned(),
                            vec![Field::new(name.to_owned(), t.0, move |ctx| {
                                let v = (|| {
                                    Ok(match ctx.parent_value.as_value().ok_or(
                                        async_graphql::Error::new(format!(
                                            "could not parse parent value of {}",
                                            name.to_owned()
                                        )),
                                    )? {
                                        async_graphql::Value::Object(o) => o.get(name.as_str()),
                                        _ => None,
                                    }
                                    .filter(|cv| async_graphql::Value::Null != **cv)
                                    .map(|cv| FieldValue::value(cv.to_owned())))
                                })();
                                FieldFuture::new(async move { v })
                            })],
                            content.2,
                            t.1,
                        ))
                    }
                    "Obj" => {
                        let content = value_to_gql_output_type(
                            name.to_owned(),
                            object.get("content").unwrap().to_owned(),
                        )?;
                        let mut o = Object::new(content.0.to_owned());
                        for f in content.1 {
                            o = o.field(f);
                        }
                        let mut objects = vec![o];
                        for obj in content.2 {
                            objects.push(obj);
                        }
                        Ok((
                            content.0.to_owned(),
                            vec![Field::new(
                                name.to_owned(),
                                TypeRef::named_nn(content.0),
                                move |ctx| {
                                    let v = (|| {
                                        Ok(match ctx.parent_value.as_value().ok_or(
                                            async_graphql::Error::new(format!(
                                                "could not parse parent value of {}",
                                                name.to_owned()
                                            )),
                                        )? {
                                            async_graphql::Value::Object(o) => o.get(name.as_str()),
                                            _ => None,
                                        }
                                        .filter(|cv| async_graphql::Value::Null != **cv)
                                        .map(|cv| FieldValue::value(cv.to_owned())))
                                    })();
                                    FieldFuture::new(async move { v })
                                },
                            )],
                            objects,
                            TypeRefKind::NamedNN,
                        ))
                    }
                    _ => return Err(format!("invalid metadata type: {}", metadata).into()),
                }
            } else {
                Err("invalid metadata content".to_string().into())
            }
        }
        e => Err(format!("invalid metadata type: {}", e).into()),
    }
}

fn listaccessor_to_value(v: ListAccessor) -> Value {
    let mut m = vec![];
    for i in v.iter() {
        m.push(valueaccessor_to_value(i));
    }
    Value::Array(m)
}

fn objectaccessor_to_value(v: ObjectAccessor) -> Value {
    let mut m = Map::new();
    for i in v.iter() {
        m.insert(i.0.to_string(), valueaccessor_to_value(i.1));
    }
    Value::Object(m)
}

fn valueaccessor_to_value(v: ValueAccessor) -> Value {
    if v.is_null() {
        Value::Null
    } else if let Ok(b) = v.boolean() {
        Value::Bool(b)
    } else if let Ok(s) = v.string() {
        Value::String(s.to_string())
    } else if let Ok(f) = v.f32() {
        serde_json::to_value(f).unwrap()
    } else if let Ok(f) = v.f64() {
        serde_json::to_value(f).unwrap()
    } else if let Ok(f) = v.i64() {
        serde_json::to_value(f).unwrap()
    } else if let Ok(f) = v.u64() {
        serde_json::to_value(f).unwrap()
    } else if let Ok(o) = v.object() {
        objectaccessor_to_value(o)
    } else if let Ok(l) = v.list() {
        listaccessor_to_value(l)
    } else {
        Value::Null
    }
}

pub fn ser_params(ctx: ResolverContext) -> Value {
    // dbg!(&ctx.field());
    let mut m = Map::new();
    let _ = ctx
        .args
        .iter()
        .map(|a| {
            m.insert(a.0.to_string(), valueaccessor_to_value(a.1));
        })
        .collect::<Vec<()>>();
    Value::Object(m)
}
