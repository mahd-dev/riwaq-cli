use std::{
    env,
    error::Error,
    ops::{Index, IndexMut},
};

use async_graphql::futures_util::TryStreamExt;
use opendal::{layers::LoggingLayer, Builder, Operator};

use wasmer::{
    Cranelift, Instance, MemoryView, Module, NativeFunc, Singlepass, Store, Universal,
    UniversalEngine, WasmPtr,
};
use wasmer_wasi::{generate_import_object_from_env, WasiEnv, WasiState};

use crate::state::State;

impl State {
    pub async fn load_wasm<B>(
        &mut self,
        _org: impl Into<String>,
        builder: B,
    ) -> Result<(), Box<dyn Error>>
    where
        B: Builder,
    {
        // let query = Object::new("Query");
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

            let wasi_env = WasiEnv::new(WasiState::new("sawi").build()?.into());

            let import_object = generate_import_object_from_env(
                &mut store,
                wasi_env,
                wasmer_wasi::WasiVersion::Snapshot1,
            );

            let instance = Instance::new(&module, &import_object).map_err(|e| dbg!(e))?;

            let msalloc: NativeFunc<u64, WasmPtr<u8>> = instance
                .exports
                .get_native_function("msalloc")
                .map_err(|e| dbg!(e))?;

            let f: NativeFunc<WasmPtr<u8>, WasmPtr<u8>> = instance
                .exports
                .get_native_function("f")
                .map_err(|e| dbg!(e))?;

            let memory = instance.exports.get_memory("memory")?;
            let memory_view: MemoryView<u8> = memory.view();

            let s = "Mohamed dardouri".to_string();

            let m = msalloc.call(s.len() as _).map_err(|e| dbg!(e))?;
            for (i, c) in s.into_bytes().iter().enumerate() {
                memory_view.index(m.offset() as usize + i).replace(*c);
            }

            let ptr = f.call(m).map_err(|e| dbg!(e))?;

            let mut data: Vec<u8> = vec![];
            for v in memory_view[(ptr.offset() as _)..].iter() {
                let v = v.get();
                if v == b'\0' {
                    break;
                }
                data.push(v);
            }
            // let data = ptr.read_until(&memory_view, |a| *a == b'\0').unwrap();
            let str = String::from_utf8_lossy(data.as_slice());
            println!("Memory contents: '{:?}'", str);

            let s = memory_view[m.offset() as _..(m.offset() + 1000) as _]
                .iter()
                .map(|v| v.get())
                .collect::<Vec<u8>>();

            dbg!(String::from_utf8_lossy(s.as_slice()), ptr, m);
        }

        // let schema = Schema::build(
        //     query.type_name(),
        //     Some(mutation.type_name()),
        //     Some(subscription.type_name()),
        // )
        // .register(query)
        // .register(mutation)
        // .register(subscription);

        // self.orgs.insert(
        //     org.into(),
        //     Org {
        //         gql: schema.finish().map_err(|e| dbg!(e))?,
        //     },
        // );

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
