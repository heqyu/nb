use std::rc::Rc;
use crate::vm::{Interpreter, Value, RuntimeError};

pub fn register(interp: &mut Interpreter) {
    // print(...)
    interp.register_native("print", |args| {
        let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
        println!("{}", parts.join("\t"));
        Ok(Value::Nil)
    });

    // type(x)
    interp.register_native("type", |args| {
        let v = args.into_iter().next().unwrap_or(Value::Nil);
        Ok(Value::Str(Rc::new(v.type_name())))
    });

    // string(x)
    interp.register_native("string", |args| {
        let v = args.into_iter().next().unwrap_or(Value::Nil);
        Ok(Value::Str(Rc::new(format!("{v}"))))
    });

    // tonumber(x)
    interp.register_native("tonumber", |args| {
        match args.into_iter().next().unwrap_or(Value::Nil) {
            Value::Number(n) => Ok(Value::Number(n)),
            Value::Str(s)    => Ok(s.parse::<f64>().map(Value::Number).unwrap_or(Value::Nil)),
            Value::Bool(b)   => Ok(Value::Number(if b { 1.0 } else { 0.0 })),
            _                => Ok(Value::Nil),
        }
    });

    // len(x)
    interp.register_native("len", |args| {
        match args.into_iter().next().unwrap_or(Value::Nil) {
            Value::Str(s)    => Ok(Value::Number(s.chars().count() as f64)),
            Value::Array(a)  => Ok(Value::Number(a.borrow().len() as f64)),
            Value::Dict(d)   => Ok(Value::Number(d.borrow().len() as f64)),
            v => Err(RuntimeError::new(format!("len 不支持 {}", v.type_name()))),
        }
    });

    // assert(x, msg)
    interp.register_native("assert", |args| {
        let cond = args.first().cloned().unwrap_or(Value::Nil);
        if !cond.is_truthy() {
            let msg = args.get(1).cloned().unwrap_or(Value::Str(Rc::new("assertion failed".into())));
            return Err(RuntimeError::new(format!("{msg}")));
        }
        Ok(Value::Nil)
    });

    // string 模块（string.format）
    interp.register_native("string", {
        |_args| Ok(Value::Nil) // placeholder，通过 string.format 访问
    });

    // 注册 string 模块 table 作为 dict
    use std::cell::RefCell;
    use indexmap::IndexMap;
    use crate::vm::ValueKey;
    let mut string_mod: IndexMap<ValueKey, Value> = IndexMap::new();
    string_mod.insert(
        ValueKey::Str("format".into()),
        Value::NativeFunction(Rc::new(|args: Vec<Value>| {
            let template = match args.first() {
                Some(Value::Str(s)) => s.clone(),
                _ => return Err(RuntimeError::new("string.format 第一个参数必须是字符串")),
            };
            let mut result = template.as_ref().clone();
            for (i, arg) in args.iter().skip(1).enumerate() {
                result = result.replace(&format!("{{{i}}}"), &format!("{arg}"));
            }
            Ok(Value::Str(Rc::new(result)))
        })),
    );
    let string_dict = Value::Dict(Rc::new(RefCell::new(string_mod)));
    interp.global.borrow_mut().define("string".to_string(), string_dict, false);
}
