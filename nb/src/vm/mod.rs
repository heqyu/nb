use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use indexmap::IndexMap;

use nb_core::parser::ast::*;

// ─────────────────────────────────────────
//  Value
// ─────────────────────────────────────────

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    Str(Rc<String>),
    Array(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<IndexMap<ValueKey, Value>>>),
    Function(Rc<Function>),
    NativeFunction(Rc<NativeFn>),
    Class(Rc<ClassObj>),
    Mixin(Rc<MixinObj>),
    Instance(Rc<RefCell<Instance>>),
    // Range 只在 for 迭代时使用
    Range(f64, f64),
}

/// Dict 的 key 类型（任意非 nil 值）
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ValueKey {
    Str(String),
    Bool(bool),
    Int(i64), // f64 转 i64 存储，方便 Hash
}

impl From<&Value> for Option<ValueKey> {
    fn from(v: &Value) -> Self {
        match v {
            Value::Str(s)  => Some(ValueKey::Str(s.as_ref().clone())),
            Value::Bool(b) => Some(ValueKey::Bool(*b)),
            Value::Number(n) => Some(ValueKey::Int((*n) as i64)),
            _ => None,
        }
    }
}

impl fmt::Display for ValueKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ValueKey::Str(s)  => write!(f, "{s}"),
            ValueKey::Bool(b) => write!(f, "{b}"),
            ValueKey::Int(n)  => write!(f, "{n}"),
        }
    }
}

pub struct Function {
    pub name: Option<String>,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub closure: Rc<RefCell<Env>>,
}

pub type NativeFn = dyn Fn(Vec<Value>) -> Result<Value, RuntimeError>;

#[derive(Clone)]
pub struct ClassObj {
    pub name: String,
    pub module: String,
    pub mixins: Vec<Rc<MixinObj>>,             // 混入的 mixin
    pub fields: Vec<FieldDef>,
    pub methods: HashMap<String, Rc<Function>>,
    pub static_methods: HashMap<String, Rc<Function>>,
}

#[derive(Clone)]
pub struct MixinObj {
    pub name: String,
    pub requires: Vec<FieldDef>,
    pub methods: HashMap<String, Rc<Function>>,
}

#[derive(Clone)]
pub struct Instance {
    pub class: Rc<ClassObj>,
    pub fields: IndexMap<String, Value>,
}

impl Instance {
    pub fn new(class: Rc<ClassObj>) -> Self {
        let mut fields = IndexMap::new();
        // 所有字段初始化为 nil
        for f in &class.fields {
            fields.insert(f.name.clone(), Value::Nil);
        }
        Self { class, fields }
    }
}

// ─────────────────────────────────────────
//  Display for Value
// ─────────────────────────────────────────

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Nil          => write!(f, "nil"),
            Value::Bool(b)      => write!(f, "{b}"),
            Value::Number(n)    => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Value::Str(s)       => write!(f, "{s}"),
            Value::Array(a)     => {
                let a = a.borrow();
                let parts: Vec<String> = a.iter().map(|v| format!("{v}")).collect();
                write!(f, "[{}]", parts.join(", "))
            }
            Value::Dict(d)      => {
                let d = d.borrow();
                let parts: Vec<String> = d.iter().map(|(k, v)| format!("{k} = {v}")).collect();
                write!(f, "{{{}}}", parts.join(", "))
            }
            Value::Function(func) => write!(f, "<fn {}>", func.name.as_deref().unwrap_or("anonymous")),
            Value::NativeFunction(_) => write!(f, "<native fn>"),
            Value::Class(c)     => write!(f, "<class {}>", c.name),
            Value::Mixin(m)     => write!(f, "<mixin {}>", m.name),
            Value::Instance(i)  => {
                let inst = i.borrow();
                // 如果有 to_string 方法则调用（在 Interpreter 中处理，这里简单输出类名）
                write!(f, "<{}>", inst.class.name)
            }
            Value::Range(s, e)  => write!(f, "{s}..{e}"),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{self}") }
}

impl Value {
    pub fn type_name(&self) -> String {
        match self {
            Value::Nil          => "nil".into(),
            Value::Bool(_)      => "bool".into(),
            Value::Number(_)    => "number".into(),
            Value::Str(_)       => "string".into(),
            Value::Array(_)     => "array".into(),
            Value::Dict(_)      => "dict".into(),
            Value::Function(_) | Value::NativeFunction(_) => "function".into(),
            Value::Class(_)     => "class".into(),
            Value::Mixin(_)     => "mixin".into(),
            Value::Instance(i)  => {
                let inst = i.borrow();
                format!("{}.{}", inst.class.module, inst.class.name)
            }
            Value::Range(_, _)  => "range".into(),
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Nil | Value::Bool(false))
    }

    pub fn is_instance_of(&self, class_name: &str) -> bool {
        match self {
            Value::Instance(i) => {
                let inst = i.borrow();
                check_instance_of(&inst.class, class_name)
            }
            _ => false,
        }
    }
}

fn check_instance_of(class: &Rc<ClassObj>, name: &str) -> bool {
    if class.name == name { return true; }
    for m in &class.mixins {
        if m.name == name { return true; }
    }
    false
}

// ─────────────────────────────────────────
//  Environment（链式作用域）
// ─────────────────────────────────────────

#[derive(Clone)]
pub struct Env {
    vars: HashMap<String, Binding>,
    parent: Option<Rc<RefCell<Env>>>,
}

#[derive(Clone)]
pub struct Binding {
    pub value: Value,
    pub mutable: bool,
}

impl Env {
    pub fn new() -> Self {
        Self { vars: HashMap::new(), parent: None }
    }

    pub fn with_parent(parent: Rc<RefCell<Env>>) -> Self {
        Self { vars: HashMap::new(), parent: Some(parent) }
    }

    pub fn define(&mut self, name: String, value: Value, mutable: bool) {
        self.vars.insert(name, Binding { value, mutable });
    }

    pub fn get(&self, name: &str) -> Option<Value> {
        if let Some(b) = self.vars.get(name) {
            return Some(b.value.clone());
        }
        self.parent.as_ref()?.borrow().get(name)
    }

    pub fn set(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        if let Some(b) = self.vars.get_mut(name) {
            if !b.mutable {
                return Err(RuntimeError::new(format!("变量 '{name}' 不可变")));
            }
            b.value = value;
            return Ok(());
        }
        if let Some(parent) = &self.parent {
            return parent.borrow_mut().set(name, value);
        }
        Err(RuntimeError::new(format!("未定义的变量 '{name}'")))
    }

    pub fn set_field(&mut self, name: &str, value: Value) -> bool {
        // 用于 ctor 中设置不可变字段（只允许一次）
        if let Some(b) = self.vars.get_mut(name) {
            b.value = value;
            return true;
        }
        false
    }

    /// 检查 name 是否在**当前**层 vars 中（不查父链），用于遮蔽检测
    pub fn vars_has_local(&self, name: &str) -> bool {
        self.vars.contains_key(name)
    }
}

// ─────────────────────────────────────────
//  ControlFlow（用于 return/break/continue/throw）
// ─────────────────────────────────────────

#[derive(Debug)]
pub enum ControlFlow {
    Return(Vec<Value>),
    Break,
    Continue,
    Throw(Value),
}

// ─────────────────────────────────────────
//  RuntimeError
// ─────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub message: String,
}

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self { message: msg.into() }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<ControlFlow> for RuntimeError {
    fn from(cf: ControlFlow) -> Self {
        match cf {
            ControlFlow::Throw(v) => RuntimeError::new(format!("{v}")),
            _ => RuntimeError::new("意外的控制流"),
        }
    }
}

// ─────────────────────────────────────────
//  Interpreter
// ─────────────────────────────────────────

pub struct Interpreter {
    pub global: Rc<RefCell<Env>>,
    pub module_name: String,
}

type ExecResult = Result<Option<ControlFlow>, RuntimeError>;
type EvalResult = Result<Value, RuntimeError>;

impl Interpreter {
    pub fn new(module_name: &str) -> Self {
        let global = Rc::new(RefCell::new(Env::new()));
        let mut interp = Self {
            global: global.clone(),
            module_name: module_name.to_string(),
        };
        crate::stdlib::register(&mut interp);
        interp
    }

    pub fn register_native(&mut self, name: &str, f: impl Fn(Vec<Value>) -> Result<Value, RuntimeError> + 'static) {
        self.global.borrow_mut().define(
            name.to_string(),
            Value::NativeFunction(Rc::new(f)),
            false,
        );
    }

    pub fn run(&mut self, stmts: &[Stmt]) -> Result<(), RuntimeError> {
        let env = self.global.clone();
        match self.exec_block(stmts, env)? {
            Some(ControlFlow::Throw(v)) => Err(RuntimeError::new(format!("{v}"))),
            _ => Ok(()),
        }
    }

    // ── 执行语句块 ──

    pub fn exec_block(&mut self, stmts: &[Stmt], env: Rc<RefCell<Env>>) -> ExecResult {
        let mut cur_env = env;
        for stmt in stmts {
            // 变量遮蔽检测：let/fn 在当前 scope 已有同名绑定时，创建子作用域
            // 这样已捕获旧绑定的闭包不受影响
            let shadow = match stmt {
                Stmt::Let { name, .. } | Stmt::FnDef(FnDef { name: Some(name), .. }) => {
                    cur_env.borrow().vars_has_local(name)
                }
                Stmt::MultiLet { names, .. } => {
                    names.iter().any(|n| cur_env.borrow().vars_has_local(n))
                }
                _ => false,
            };
            if shadow {
                cur_env = Rc::new(RefCell::new(Env::with_parent(cur_env)));
            }
            if let Some(cf) = self.exec_stmt(stmt, cur_env.clone())? {
                return Ok(Some(cf));
            }
        }
        Ok(None)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: Rc<RefCell<Env>>) -> ExecResult {
        match stmt {
            Stmt::Let { name, mutable, value, .. } => {
                let val = match value {
                    Some(e) => self.eval_multi(e, env.clone())?.into_iter().next().unwrap_or(Value::Nil),
                    None    => Value::Nil,
                };
                env.borrow_mut().define(name.clone(), val, *mutable);
                Ok(None)
            }

            Stmt::MultiLet { names, mutable, value, .. } => {
                let vals = match value {
                    Some(e) => self.eval_multi(e, env.clone())?,
                    None    => vec![Value::Nil],
                };
                for (i, name) in names.iter().enumerate() {
                    let v = vals.get(i).cloned().unwrap_or(Value::Nil);
                    env.borrow_mut().define(name.clone(), v, *mutable);
                }
                Ok(None)
            }

            Stmt::Assign { target, value } => {
                let val = self.eval(value, env.clone())?;
                self.assign(target, val, env, false)?;
                Ok(None)
            }

            Stmt::CompoundAssign { target, op, value } => {
                let right = self.eval(value, env.clone())?;
                let left  = self.eval(target, env.clone())?;
                let result = self.apply_binop(op, left, right)?;
                self.assign(target, result, env, false)?;
                Ok(None)
            }

            Stmt::IncDec { target, inc } => {
                let val = self.eval(target, env.clone())?;
                let result = match val {
                    Value::Number(n) => Value::Number(if *inc { n + 1.0 } else { n - 1.0 }),
                    _ => return Err(RuntimeError::new("++ / -- 只能用于 number")),
                };
                self.assign(target, result, env, false)?;
                Ok(None)
            }

            Stmt::FnDef(fndef) => {
                let func = Rc::new(Function {
                    name: fndef.name.clone(),
                    params: fndef.params.clone(),
                    body: fndef.body.clone(),
                    closure: env.clone(),
                });
                if let Some(receiver) = &fndef.receiver {
                    // fn Player.method(...) → 挂载到已定义的 class 上
                    let class_val = env.borrow().get(receiver)
                        .ok_or_else(|| RuntimeError::new(format!("未定义的类 '{receiver}'")))?;
                    match class_val {
                        Value::Class(class_rc) => {
                            let method_name = fndef.name.clone().unwrap_or_default();
                            // ClassObj.methods 是 HashMap，需要内部可变性
                            // 用 Rc<RefCell<ClassObj>> 或直接重建；这里用 unsafe 强转绕开
                            // 更好的方式：ClassObj.methods 改为 RefCell
                            // 暂时：重建一个新的 ClassObj 替换环境中的绑定
                            let mut new_methods = class_rc.methods.clone();
                            new_methods.insert(method_name, func);
                            let new_class = Rc::new(ClassObj {
                                name: class_rc.name.clone(),
                                module: class_rc.module.clone(),
                                mixins: class_rc.mixins.clone(),
                                fields: class_rc.fields.clone(),
                                methods: new_methods,
                                static_methods: class_rc.static_methods.clone(),
                            });
                            env.borrow_mut().define(receiver.clone(), Value::Class(new_class), false);
                        }
                        _ => return Err(RuntimeError::new(format!("'{receiver}' 不是一个类"))),
                    }
                } else {
                    let name = fndef.name.clone().unwrap_or_default();
                    env.borrow_mut().define(name, Value::Function(func), false);
                }
                Ok(None)
            }

            Stmt::Return(expr) => {
                let vals = match expr {
                    Some(e) => self.eval_multi(e, env)?,
                    None    => vec![],
                };
                Ok(Some(ControlFlow::Return(vals)))
            }

            Stmt::If { cond, then_body, else_ifs, else_body, .. } => {
                let c = self.eval(cond, env.clone())?;
                if c.is_truthy() {
                    let child = Rc::new(RefCell::new(Env::with_parent(env)));
                    return self.exec_block(then_body, child);
                }
                for (ec, eb) in else_ifs {
                    let c = self.eval(ec, env.clone())?;
                    if c.is_truthy() {
                        let child = Rc::new(RefCell::new(Env::with_parent(env)));
                        return self.exec_block(eb, child);
                    }
                }
                if let Some(eb) = else_body {
                    let child = Rc::new(RefCell::new(Env::with_parent(env)));
                    return self.exec_block(eb, child);
                }
                Ok(None)
            }

            Stmt::While { cond, body, .. } => {
                loop {
                    let c = self.eval(cond, env.clone())?;
                    if !c.is_truthy() { break; }
                    let child = Rc::new(RefCell::new(Env::with_parent(env.clone())));
                    match self.exec_block(body, child)? {
                        Some(ControlFlow::Break)    => break,
                        Some(ControlFlow::Continue) => continue,
                        Some(cf)                    => return Ok(Some(cf)),
                        None                        => {}
                    }
                }
                Ok(None)
            }

            Stmt::ForIn { key, value, value_mutable, iter, body, .. } => {
                self.exec_for(key, value.as_deref(), *value_mutable, iter, body, env)
            }

            Stmt::Break    => Ok(Some(ControlFlow::Break)),
            Stmt::Continue => Ok(Some(ControlFlow::Continue)),

            Stmt::ClassDef(cd) => {
                self.define_class(cd, env)?;
                Ok(None)
            }

            Stmt::MixinDef(md) => {
                self.define_mixin(md, env)?;
                Ok(None)
            }

            Stmt::Throw(expr) => {
                let val = self.eval(expr, env)?;
                Ok(Some(ControlFlow::Throw(val)))
            }

            Stmt::Export(_) => Ok(None), // 模块系统后续实现

            Stmt::Expr(expr) => {
                self.eval(expr, env)?;
                Ok(None)
            }
        }
    }

    // ── For 循环 ──

    fn exec_for(&mut self, key: &str, value: Option<&str>, val_mut: bool,
                iter_expr: &Expr, body: &[Stmt], env: Rc<RefCell<Env>>) -> ExecResult {
        let iter_val = self.eval(iter_expr, env.clone())?;
        match &iter_val {
            Value::Range(start, end) => {
                let (s, e) = (*start as i64, *end as i64);
                for i in s..e {
                    let child = Rc::new(RefCell::new(Env::with_parent(env.clone())));
                    child.borrow_mut().define(key.to_string(), Value::Number(i as f64), false);
                    match self.exec_block(body, child)? {
                        Some(ControlFlow::Break)    => break,
                        Some(ControlFlow::Continue) => continue,
                        Some(cf)                    => return Ok(Some(cf)),
                        None                        => {}
                    }
                }
            }
            Value::Array(arr) => {
                let items: Vec<Value> = arr.borrow().clone();
                for (i, v) in items.iter().enumerate() {
                    let child = Rc::new(RefCell::new(Env::with_parent(env.clone())));
                    child.borrow_mut().define(key.to_string(), Value::Number(i as f64), false);
                    if let Some(vname) = value {
                        child.borrow_mut().define(vname.to_string(), v.clone(), val_mut);
                    }
                    match self.exec_block(body, child)? {
                        Some(ControlFlow::Break)    => break,
                        Some(ControlFlow::Continue) => continue,
                        Some(cf)                    => return Ok(Some(cf)),
                        None                        => {}
                    }
                }
            }
            Value::Dict(dict) => {
                let pairs: Vec<(ValueKey, Value)> = dict.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                for (k, v) in pairs {
                    let child = Rc::new(RefCell::new(Env::with_parent(env.clone())));
                    child.borrow_mut().define(key.to_string(), Value::Str(Rc::new(k.to_string())), false);
                    if let Some(vname) = value {
                        child.borrow_mut().define(vname.to_string(), v, val_mut);
                    }
                    match self.exec_block(body, child)? {
                        Some(ControlFlow::Break)    => break,
                        Some(ControlFlow::Continue) => continue,
                        Some(cf)                    => return Ok(Some(cf)),
                        None                        => {}
                    }
                }
            }
            Value::Str(s) => {
                let chars: Vec<char> = s.chars().collect();
                for (i, c) in chars.iter().enumerate() {
                    let child = Rc::new(RefCell::new(Env::with_parent(env.clone())));
                    child.borrow_mut().define(key.to_string(), Value::Number(i as f64), false);
                    if let Some(vname) = value {
                        child.borrow_mut().define(vname.to_string(), Value::Str(Rc::new(c.to_string())), val_mut);
                    }
                    match self.exec_block(body, child)? {
                        Some(ControlFlow::Break)    => break,
                        Some(ControlFlow::Continue) => continue,
                        Some(cf)                    => return Ok(Some(cf)),
                        None                        => {}
                    }
                }
            }
            _ => return Err(RuntimeError::new(format!("无法迭代: {}", iter_val.type_name()))),
        }
        Ok(None)
    }

    // ── 赋值 ──

    fn assign(&mut self, target: &Expr, val: Value, env: Rc<RefCell<Env>>, in_ctor: bool) -> Result<(), RuntimeError> {
        match target {
            Expr::Ident(name, _) => {
                env.borrow_mut().set(name, val)?;
            }
            Expr::Field { obj, field, .. } => {
                let obj_val = self.eval(obj, env.clone())?;
                match obj_val {
                    Value::Instance(inst) => {
                        let mut inst = inst.borrow_mut();
                        // ctor 内可以设置不可变字段，其他地方只能设置 mut 字段
                        let field_def = inst.class.fields.iter().find(|f| f.name == *field);
                        if let Some(fd) = field_def {
                            if !fd.mutable && !in_ctor {
                                return Err(RuntimeError::new(format!("字段 '{field}' 不可变")));
                            }
                        }
                        inst.fields.insert(field.clone(), val);
                    }
                    _ => return Err(RuntimeError::new(format!("无法设置字段 '{field}'"))),
                }
            }
            Expr::Index { obj, idx } => {
                let obj_val = self.eval(obj, env.clone())?;
                let idx_val = self.eval(idx, env)?;
                match obj_val {
                    Value::Array(arr) => {
                        let i = self.to_index(&idx_val)?;
                        let mut arr = arr.borrow_mut();
                        if i >= arr.len() {
                            return Err(RuntimeError::new(format!("数组下标越界: {i}")));
                        }
                        arr[i] = val;
                    }
                    Value::Dict(dict) => {
                        let key = Option::<ValueKey>::from(&idx_val)
                            .ok_or_else(|| RuntimeError::new("dict key 不能为 nil"))?;
                        dict.borrow_mut().insert(key, val);
                    }
                    _ => return Err(RuntimeError::new("无法索引赋值")),
                }
            }
            _ => return Err(RuntimeError::new("无效的赋值目标")),
        }
        Ok(())
    }

    fn to_index(&self, v: &Value) -> Result<usize, RuntimeError> {
        match v {
            Value::Number(n) => Ok(*n as usize),
            _ => Err(RuntimeError::new("数组下标必须是 number")),
        }
    }

    // ── 表达式求值 ──

    pub fn eval(&mut self, expr: &Expr, env: Rc<RefCell<Env>>) -> EvalResult {
        match expr {
            Expr::Nil         => Ok(Value::Nil),
            Expr::Bool(b)     => Ok(Value::Bool(*b)),
            Expr::Number(n)   => Ok(Value::Number(*n)),
            Expr::StringLit(s) => Ok(Value::Str(Rc::new(s.clone()))),

            Expr::InterpolatedString(parts) => {
                let mut result = String::new();
                for part in parts {
                    match part {
                        InterpPart::Literal(s) => result.push_str(s),
                        InterpPart::Expr(expr) => {
                            let val = self.eval(expr, env.clone())?;
                            result.push_str(&format!("{val}"));
                        }
                    }
                }
                Ok(Value::Str(Rc::new(result)))
            }

            Expr::Ident(name, _) => {
                env.borrow().get(name)
                    .ok_or_else(|| RuntimeError::new(format!("未定义的变量 '{name}'")))
            }

            Expr::BinOp { left, op, right } => {
                // && 和 || 短路求值
                match op {
                    BinOp::And => {
                        let l = self.eval(left, env.clone())?;
                        if !l.is_truthy() { return Ok(Value::Bool(false)); }
                        let r = self.eval(right, env)?;
                        return Ok(Value::Bool(r.is_truthy()));
                    }
                    BinOp::Or => {
                        let l = self.eval(left, env.clone())?;
                        if l.is_truthy() { return Ok(Value::Bool(true)); }
                        let r = self.eval(right, env)?;
                        return Ok(Value::Bool(r.is_truthy()));
                    }
                    BinOp::Range => {
                        let l = self.eval(left, env.clone())?;
                        let r = self.eval(right, env)?;
                        match (l, r) {
                            (Value::Number(s), Value::Number(e)) => return Ok(Value::Range(s, e)),
                            _ => return Err(RuntimeError::new("range 两端必须是 number")),
                        }
                    }
                    _ => {}
                }
                let l = self.eval(left, env.clone())?;
                let r = self.eval(right, env)?;
                self.apply_binop(op, l, r)
            }

            Expr::UnaryOp { op, expr } => {
                let v = self.eval(expr, env)?;
                match op {
                    UnaryOp::Neg => match v {
                        Value::Number(n) => Ok(Value::Number(-n)),
                        _ => Err(RuntimeError::new("一元 - 只能用于 number")),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!v.is_truthy())),
                }
            }

            Expr::Ternary { cond, then, else_ } => {
                let c = self.eval(cond, env.clone())?;
                if c.is_truthy() { self.eval(then, env) } else { self.eval(else_, env) }
            }

            Expr::Array(elems) => {
                let mut arr = Vec::new();
                for e in elems { arr.push(self.eval(e, env.clone())?); }
                Ok(Value::Array(Rc::new(RefCell::new(arr))))
            }

            Expr::Dict(pairs) => {
                let mut dict = IndexMap::new();
                for (k, v) in pairs {
                    // dict 字面量中 Ident key 自动转为字符串（{ name = 1 } 中的 name）
                    let kv = match k {
                        Expr::Ident(s, _) => Value::Str(Rc::new(s.clone())),
                        _ => self.eval(k, env.clone())?,
                    };
                    let vv = self.eval(v, env.clone())?;
                    let key = Option::<ValueKey>::from(&kv)
                        .ok_or_else(|| RuntimeError::new("dict key 不能为 nil"))?;
                    dict.insert(key, vv);
                }
                Ok(Value::Dict(Rc::new(RefCell::new(dict))))
            }

            Expr::Fn(fndef) => {
                Ok(Value::Function(Rc::new(Function {
                    name: fndef.name.clone(),
                    params: fndef.params.clone(),
                    body: fndef.body.clone(),
                    closure: env,
                })))
            }

            Expr::Call { callee, args, .. } => {
                // 特殊处理方法调用：obj.method(args)
                if let Expr::Field { obj, field, .. } = callee.as_ref() {
                    let obj_val = self.eval(obj, env.clone())?;
                    let mut arg_vals = Vec::new();
                    for a in args { arg_vals.push(self.eval(&a.expr, env.clone())?); }
                    return self.call_method(obj_val, field, arg_vals);
                }
                // 普通函数调用
                let func_val = self.eval(callee, env.clone())?;
                let mut arg_vals = Vec::new();
                for a in args { arg_vals.push(self.eval(&a.expr, env.clone())?); }
                self.call_value(func_val, arg_vals, None)
            }

            Expr::Field { obj, field, .. } => {
                let obj_val = self.eval(obj, env.clone())?;
                self.get_field(obj_val, field, env)
            }

            Expr::Index { obj, idx } => {
                let obj_val = self.eval(obj, env.clone())?;
                let idx_val = self.eval(idx, env)?;
                self.get_index(obj_val, idx_val)
            }

            Expr::StructLit { class, fields, .. } => {
                let class_val = env.borrow().get(class)
                    .ok_or_else(|| RuntimeError::new(format!("未定义的类 '{class}'")))?;
                let class_rc = match class_val {
                    Value::Class(c) => c,
                    _ => return Err(RuntimeError::new(format!("'{class}' 不是一个类"))),
                };
                let inst = Rc::new(RefCell::new(Instance::new(class_rc)));
                // 按字段名初始化，缺字段保持 nil
                for (fname, _, fexpr) in fields {
                    let val = self.eval(fexpr, env.clone())?;
                    inst.borrow_mut().fields.insert(fname.clone(), val);
                }
                Ok(Value::Instance(inst))
            }

            Expr::Is { expr, type_name, .. } => {
                let v = self.eval(expr, env.clone())?;
                // 检查类、trait
                let result = match &v {
                    Value::Instance(_) => v.is_instance_of(type_name),
                    _ => v.type_name() == *type_name,
                };
                Ok(Value::Bool(result))
            }

            Expr::Protect(body) => {
                self.exec_protect(body, env)
            }

            Expr::Try(expr) => {
                // ? 操作符：出错则向上传播 ControlFlow::Throw
                // 在函数调用时处理，这里只求值
                self.eval(expr, env)
            }

            Expr::Await(expr) => {
                // 暂不支持异步，直接求值
                self.eval(expr, env)
            }
        }
    }

    // ── protect 块 ──

    fn exec_protect(&mut self, body: &[Stmt], env: Rc<RefCell<Env>>) -> EvalResult {
        let child = Rc::new(RefCell::new(Env::with_parent(env)));
        match self.exec_block(body, child) {
            Ok(Some(ControlFlow::Return(vals))) => {
                // 返回 [nil, val1, val2, ...]
                let mut result = vec![Value::Nil];
                result.extend(vals);
                Ok(Value::Array(Rc::new(RefCell::new(result))))
            }
            Ok(Some(ControlFlow::Throw(v))) => {
                // 返回 [err, nil]
                Ok(Value::Array(Rc::new(RefCell::new(vec![v, Value::Nil]))))
            }
            Ok(_) => {
                // 无返回值，返回 [nil]
                Ok(Value::Array(Rc::new(RefCell::new(vec![Value::Nil]))))
            }
            Err(e) => {
                // 运行时错误也捕获
                let err_val = Value::Str(Rc::new(e.message));
                Ok(Value::Array(Rc::new(RefCell::new(vec![err_val, Value::Nil]))))
            }
        }
    }

    // ── eval_multi（用于多返回值）──

    pub fn eval_multi(&mut self, expr: &Expr, env: Rc<RefCell<Env>>) -> Result<Vec<Value>, RuntimeError> {
        // protect 返回数组，展开为多个值
        match expr {
            Expr::Protect(_) => {
                let v = self.eval(expr, env)?;
                match v {
                    Value::Array(arr) => Ok(arr.borrow().clone()),
                    other => Ok(vec![other]),
                }
            }
            _ => Ok(vec![self.eval(expr, env)?]),
        }
    }

    // ── 方法调用（正确传递 self）──

    fn call_method(&mut self, obj: Value, method: &str, args: Vec<Value>) -> EvalResult {
        match &obj {
            Value::Instance(inst_rc) => {
                // 先查实例字段（可能是函数）
                let field_val = inst_rc.borrow().fields.get(method).cloned();
                if let Some(v) = field_val {
                    return self.call_value(v, args, None);
                }
                // 查方法
                let class = inst_rc.borrow().class.clone();
                if let Some(m) = find_method(&class, method) {
                    return self.call_function(&m, args, Some(obj.clone()));
                }
                Err(RuntimeError::new(format!("实例没有方法 '{method}'")))
            }
            Value::Class(class) => {
                if let Some(m) = class.static_methods.get(method) {
                    return self.call_function(m, args, None);
                }
                Err(RuntimeError::new(format!("类 '{}' 没有静态方法 '{method}'", class.name)))
            }
            Value::Str(s) => {
                string_method_call(s, method, args)
            }
            Value::Array(arr) => {
                // 需要回调的方法在这里处理
                self.array_method_with_cb(arr.clone(), method, args)
            }
            Value::Dict(dict) => {
                // 先查 dict 里是否有这个 key（用于模块 dict，如 string.format）
                let key = ValueKey::Str(method.to_string());
                if let Some(func_val) = dict.borrow().get(&key).cloned() {
                    return self.call_value(func_val, args, None);
                }
                dict_method_call(dict.clone(), method, args)
            }
            _ => Err(RuntimeError::new(format!("无法在 {} 上调用方法 '{method}'", obj.type_name()))),
        }
    }

    fn array_method_with_cb(&mut self, arr: Rc<RefCell<Vec<Value>>>, method: &str, args: Vec<Value>) -> EvalResult {
        match method {
            "map" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("map 需要一个函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                let mut result = Vec::new();
                for v in items {
                    result.push(self.call_value(cb.clone(), vec![v], None)?);
                }
                Ok(Value::Array(Rc::new(RefCell::new(result))))
            }
            "filter" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("filter 需要一个函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                let mut result = Vec::new();
                for v in items {
                    let keep = self.call_value(cb.clone(), vec![v.clone()], None)?;
                    if keep.is_truthy() { result.push(v); }
                }
                Ok(Value::Array(Rc::new(RefCell::new(result))))
            }
            "reduce" => {
                let mut args = args.into_iter();
                let cb   = args.next().ok_or_else(|| RuntimeError::new("reduce 需要回调参数"))?;
                let init = args.next().unwrap_or(Value::Nil);
                let items: Vec<Value> = arr.borrow().clone();
                let mut acc = init;
                for v in items {
                    acc = self.call_value(cb.clone(), vec![acc, v], None)?;
                }
                Ok(acc)
            }
            "find" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("find 需要函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                for v in items {
                    let ok = self.call_value(cb.clone(), vec![v.clone()], None)?;
                    if ok.is_truthy() { return Ok(v); }
                }
                Ok(Value::Nil)
            }
            "find_index" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("find_index 需要函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                for (i, v) in items.iter().enumerate() {
                    let ok = self.call_value(cb.clone(), vec![v.clone()], None)?;
                    if ok.is_truthy() { return Ok(Value::Number(i as f64)); }
                }
                Ok(Value::Number(-1.0))
            }
            "every" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("every 需要函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                for v in items {
                    let ok = self.call_value(cb.clone(), vec![v], None)?;
                    if !ok.is_truthy() { return Ok(Value::Bool(false)); }
                }
                Ok(Value::Bool(true))
            }
            "some" => {
                let cb = args.into_iter().next().ok_or_else(|| RuntimeError::new("some 需要函数参数"))?;
                let items: Vec<Value> = arr.borrow().clone();
                for v in items {
                    let ok = self.call_value(cb.clone(), vec![v], None)?;
                    if ok.is_truthy() { return Ok(Value::Bool(true)); }
                }
                Ok(Value::Bool(false))
            }
            "sort" => {
                if let Some(cb) = args.into_iter().next() {
                    // 自定义比较（简化版：不支持运行时错误传播）
                    let items: Vec<Value> = arr.borrow().clone();
                    let mut result = items;
                    // 用冒泡排序避免闭包借用问题
                    let n = result.len();
                    for i in 0..n {
                        for j in 0..n-i-1 {
                            let cmp = self.call_value(cb.clone(), vec![result[j].clone(), result[j+1].clone()], None)?;
                            if let Value::Number(n) = cmp {
                                if n > 0.0 { result.swap(j, j+1); }
                            }
                        }
                    }
                    Ok(Value::Array(Rc::new(RefCell::new(result))))
                } else {
                    array_method_call(arr, "sort", vec![])
                }
            }
            // 其余不需要回调的方法委托给 array_method_call
            _ => array_method_call(arr, method, args),
        }
    }

    // ── 内置字段访问 ──

    fn get_field(&mut self, obj: Value, field: &str, env: Rc<RefCell<Env>>) -> EvalResult {
        match &obj {
            Value::Instance(inst_rc) => {
                // 先查实例字段
                if let Some(v) = inst_rc.borrow().fields.get(field) {
                    return Ok(v.clone());
                }
                // 再查方法
                let class = inst_rc.borrow().class.clone();
                if let Some(method) = find_method(&class, field) {
                    // 绑定 self
                    let bound = make_bound_method(method, obj.clone());
                    return Ok(bound);
                }
                Err(RuntimeError::new(format!("实例没有字段或方法 '{field}'")))
            }
            Value::Class(class) => {
                if let Some(m) = class.static_methods.get(field) {
                    return Ok(Value::Function(m.clone()));
                }
                Err(RuntimeError::new(format!("类 '{}' 没有静态方法 '{field}'", class.name)))
            }
            Value::Str(s) => self.string_method(s.clone(), field, env),
            Value::Array(arr) => self.array_method(arr.clone(), field, env),
            Value::Dict(dict) => self.dict_method(dict.clone(), field, env),
            _ => Err(RuntimeError::new(format!("无法访问 {} 的字段 '{field}'", obj.type_name()))),
        }
    }

    fn get_index(&mut self, obj: Value, idx: Value) -> EvalResult {
        match &obj {
            Value::Array(arr) => {
                let i = self.to_index(&idx)?;
                let arr = arr.borrow();
                arr.get(i).cloned().ok_or_else(|| RuntimeError::new(format!("数组下标越界: {i}")))
            }
            Value::Dict(dict) => {
                let key = Option::<ValueKey>::from(&idx)
                    .ok_or_else(|| RuntimeError::new("dict key 不能为 nil"))?;
                Ok(dict.borrow().get(&key).cloned().unwrap_or(Value::Nil))
            }
            Value::Str(s) => {
                let i = self.to_index(&idx)?;
                let c = s.chars().nth(i)
                    .ok_or_else(|| RuntimeError::new(format!("字符串下标越界: {i}")))?;
                Ok(Value::Str(Rc::new(c.to_string())))
            }
            _ => Err(RuntimeError::new(format!("无法索引 {}", obj.type_name()))),
        }
    }

    // ── 函数调用 ──

    pub fn call_value(&mut self, func: Value, args: Vec<Value>, this: Option<Value>) -> EvalResult {
        match func {
            Value::Function(f) => self.call_function(&f, args, this),
            Value::NativeFunction(nf) => nf(args),
            Value::Class(c) => self.instantiate(Value::Class(c), args),
            _ => Err(RuntimeError::new(format!("无法调用 {}", func.type_name()))),
        }
    }

    fn call_function(&mut self, func: &Rc<Function>, args: Vec<Value>, this: Option<Value>) -> EvalResult {
        let call_env = Rc::new(RefCell::new(Env::with_parent(func.closure.clone())));

        // 绑定 self
        if let Some(self_val) = this {
            call_env.borrow_mut().define("self".to_string(), self_val, true);
        }

        // 跳过 self 参数，其余参数按顺序绑定
        let non_self_params: Vec<&Param> = func.params.iter().filter(|p| p.name != "self").collect();
        for (i, param) in non_self_params.iter().enumerate() {
            let v = args.get(i).cloned().unwrap_or(Value::Nil);
            call_env.borrow_mut().define(param.name.clone(), v, param.mutable);
        }

        match self.exec_block(&func.body, call_env)? {
            Some(ControlFlow::Return(vals)) => Ok(vals.into_iter().next().unwrap_or(Value::Nil)),
            Some(ControlFlow::Throw(v))     => Err(RuntimeError::new(format!("{v}"))),
            _                               => Ok(Value::Nil),
        }
    }

    // ── 实例化 ──

    fn instantiate(&mut self, class_val: Value, args: Vec<Value>) -> EvalResult {
        let class = match class_val {
            Value::Class(c) => c,
            _ => return Err(RuntimeError::new("new 后面必须是一个类")),
        };

        let inst = Rc::new(RefCell::new(Instance::new(class.clone())));
        let inst_val = Value::Instance(inst.clone());

        // 调用自身 ctor
        if let Some(ctor) = class.methods.get("ctor") {
            self.call_ctor(ctor.clone(), inst_val.clone(), args)?;
        }

        Ok(inst_val)
    }

    fn call_ctor(&mut self, ctor: Rc<Function>, inst: Value, args: Vec<Value>) -> Result<(), RuntimeError> {
        let call_env = Rc::new(RefCell::new(Env::with_parent(ctor.closure.clone())));
        call_env.borrow_mut().define("self".to_string(), inst.clone(), true);

        // 跳过 self 参数，其余参数按顺序绑定
        let non_self_params: Vec<&Param> = ctor.params.iter().filter(|p| p.name != "self").collect();
        for (i, param) in non_self_params.iter().enumerate() {
            let v = args.get(i).cloned().unwrap_or(Value::Nil);
            call_env.borrow_mut().define(param.name.clone(), v, param.mutable);
        }

        // ctor 中对 self.field 的赋值需要允许不可变字段
        self.exec_block_in_ctor(&ctor.body, call_env, &inst)?;
        Ok(())
    }

    fn exec_block_in_ctor(&mut self, stmts: &[Stmt], env: Rc<RefCell<Env>>, inst: &Value) -> ExecResult {
        for stmt in stmts {
            // 处理 self.field = val 赋值（允许不可变字段）
            let is_self_field_assign = match stmt {
                Stmt::Assign { target: Expr::Field { obj, field: _, .. }, value: _ } => {
                    matches!(obj.as_ref(), Expr::Ident(n, _) if n == "self")
                }
                _ => false,
            };
            if is_self_field_assign {
                if let Stmt::Assign { target: Expr::Field { obj: _, field, .. }, value } = stmt {
                    let val = self.eval(value, env.clone())?;
                    if let Value::Instance(inst_rc) = inst {
                        inst_rc.borrow_mut().fields.insert(field.clone(), val);
                        continue;
                    }
                }
            }
            if let Some(cf) = self.exec_stmt(stmt, env.clone())? {
                return Ok(Some(cf));
            }
        }
        Ok(None)
    }

    // ── class/mixin 定义 ──

    fn define_class(&mut self, cd: &ClassDef, env: Rc<RefCell<Env>>) -> Result<(), RuntimeError> {
        let mut mixins: Vec<Rc<MixinObj>> = Vec::new();

        for mname in &cd.mixins {
            match env.borrow().get(mname) {
                Some(Value::Mixin(m)) => mixins.push(m),
                Some(_) => return Err(RuntimeError::new(format!("'{mname}' 不是一个 mixin"))),
                None    => return Err(RuntimeError::new(format!("未定义的 mixin '{mname}'"))),
            }
        }

        // require 静态检查：验证 class 声明了 mixin 所需的全部字段
        let class_field_names: Vec<&str> = cd.fields.iter().map(|f| f.name.as_str()).collect();
        for m_rc in &mixins {
            for req in &m_rc.requires {
                if !class_field_names.contains(&req.name.as_str()) {
                    return Err(RuntimeError::new(format!(
                        "class '{}' 混入 '{}' 但缺少必需字段 '{}'",
                        cd.name, m_rc.name, req.name
                    )));
                }
            }
        }

        // 收集字段
        let all_fields = cd.fields.clone();

        // 方法通过 fn ClassName.method 在类定义后注册，这里初始化时先混入 mixin 方法
        let mut methods: HashMap<String, Rc<Function>> = HashMap::new();

        for m in &mixins {
            for (name, func) in &m.methods {
                methods.insert(name.clone(), func.clone());
            }
        }

        let class_obj = Rc::new(ClassObj {
            name: cd.name.clone(),
            module: self.module_name.clone(),
            mixins,
            fields: all_fields,
            methods,
            static_methods: HashMap::new(),
        });

        env.borrow_mut().define(cd.name.clone(), Value::Class(class_obj), false);
        Ok(())
    }

    fn define_mixin(&mut self, md: &MixinDef, env: Rc<RefCell<Env>>) -> Result<(), RuntimeError> {
        let mut methods = HashMap::new();
        for m in &md.methods {
            let func = Rc::new(Function {
                name: m.name.clone(),
                params: m.params.clone(),
                body: m.body.clone(),
                closure: env.clone(),
            });
            methods.insert(m.name.clone().unwrap_or_default(), func);
        }
        let mixin_obj = Rc::new(MixinObj {
            name: md.name.clone(),
            requires: md.requires.clone(),
            methods,
        });
        env.borrow_mut().define(md.name.clone(), Value::Mixin(mixin_obj), false);
        Ok(())
    }

    // ── 二元运算 ──

    fn apply_binop(&self, op: &BinOp, l: Value, r: Value) -> EvalResult {
        match op {
            BinOp::Add => match (&l, &r) {
                (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                (Value::Str(a), Value::Str(b))       => Ok(Value::Str(Rc::new(format!("{a}{b}")))),
                _ => Err(RuntimeError::new(format!("无法对 {} 和 {} 执行 +", l.type_name(), r.type_name()))),
            },
            BinOp::Sub => num_op!(l, r, -, "−"),
            BinOp::Mul => num_op!(l, r, *, "*"),
            BinOp::Div => {
                match (&l, &r) {
                    (Value::Number(a), Value::Number(b)) => {
                        if *b == 0.0 { return Err(RuntimeError::new("除数为零")); }
                        Ok(Value::Number(a / b))
                    }
                    _ => Err(RuntimeError::new("/ 只能用于 number")),
                }
            }
            BinOp::Mod => num_op!(l, r, %, "%"),
            BinOp::Eq    => Ok(Value::Bool(values_eq(&l, &r))),
            BinOp::NotEq => Ok(Value::Bool(!values_eq(&l, &r))),
            BinOp::Lt    => Ok(Value::Bool(cmp_values(&l, &r)? < 0)),
            BinOp::LtEq  => Ok(Value::Bool(cmp_values(&l, &r)? <= 0)),
            BinOp::Gt    => Ok(Value::Bool(cmp_values(&l, &r)? > 0)),
            BinOp::GtEq  => Ok(Value::Bool(cmp_values(&l, &r)? >= 0)),
            BinOp::And | BinOp::Or | BinOp::Range => unreachable!(),
        }
    }

    // ── 字符串/Array/Dict 方法 ──

    fn string_method(&mut self, s: Rc<String>, method: &str, _env: Rc<RefCell<Env>>) -> EvalResult {
        let s_clone = s.clone();
        match method {
            "len"         => Ok(Value::Number(s.chars().count() as f64)),
            "upper"       => Ok(Value::Str(Rc::new(s.to_uppercase()))),
            "lower"       => Ok(Value::Str(Rc::new(s.to_lowercase()))),
            "trim"        => Ok(Value::Str(Rc::new(s.trim().to_string()))),
            _ => {
                // 返回 native closure，懒处理
                let s2 = s_clone.clone();
                let method = method.to_string();
                Ok(Value::NativeFunction(Rc::new(move |args: Vec<Value>| {
                    string_method_call(&s2, &method, args)
                })))
            }
        }
    }

    fn array_method(&mut self, arr: Rc<RefCell<Vec<Value>>>, method: &str, _env: Rc<RefCell<Env>>) -> EvalResult {
        let arr2 = arr.clone();
        let method = method.to_string();
        Ok(Value::NativeFunction(Rc::new(move |args: Vec<Value>| {
            array_method_call(arr2.clone(), &method, args)
        })))
    }

    fn dict_method(&mut self, dict: Rc<RefCell<IndexMap<ValueKey, Value>>>, method: &str, _env: Rc<RefCell<Env>>) -> EvalResult {
        let dict2 = dict.clone();
        let method = method.to_string();
        Ok(Value::NativeFunction(Rc::new(move |args: Vec<Value>| {
            dict_method_call(dict2.clone(), &method, args)
        })))
    }
}

// ─────────────────────────────────────────
//  辅助宏和函数
// ─────────────────────────────────────────

macro_rules! num_op {
    ($l:expr, $r:expr, $op:tt, $name:expr) => {
        match (&$l, &$r) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a $op b)),
            _ => Err(RuntimeError::new(format!("{} 只能用于 number", $name))),
        }
    };
}
use num_op;

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Nil, Value::Nil)         => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => x == y,
        (Value::Str(x), Value::Str(y))   => x == y,
        _ => false,
    }
}

fn cmp_values(a: &Value, b: &Value) -> Result<i32, RuntimeError> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => Ok(if x < y { -1 } else if x > y { 1 } else { 0 }),
        (Value::Str(x), Value::Str(y))       => Ok(x.cmp(y) as i32),
        _ => Err(RuntimeError::new(format!("无法比较 {} 和 {}", a.type_name(), b.type_name()))),
    }
}

fn find_method(class: &Rc<ClassObj>, name: &str) -> Option<Rc<Function>> {
    class.methods.get(name).cloned()
}

fn make_bound_method(method: Rc<Function>, this: Value) -> Value {
    Value::NativeFunction(Rc::new(move |args: Vec<Value>| {
        // 无法直接调用，需要 Interpreter，通过 Instance 的方法分发处理
        // 这里创建一个特殊的 BoundMethod value — 用 NativeFunction 包裹不够
        // 实际调用在 Interpreter::eval 的 Call 分支处理
        let _ = (args, &this, &method);
        Ok(Value::Nil)
    }))
}

// ─────────────────────────────────────────
//  字符串方法实现
// ─────────────────────────────────────────

fn string_method_call(s: &str, method: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    match method {
        "len"         => Ok(Value::Number(s.chars().count() as f64)),
        "upper"       => Ok(Value::Str(Rc::new(s.to_uppercase()))),
        "lower"       => Ok(Value::Str(Rc::new(s.to_lowercase()))),
        "trim"        => Ok(Value::Str(Rc::new(s.trim().to_string()))),
        "split"       => {
            let sep = str_arg(&args, 0, "split")?;
            let parts: Vec<Value> = s.split(sep.as_str())
                .map(|p| Value::Str(Rc::new(p.to_string()))).collect();
            Ok(Value::Array(Rc::new(RefCell::new(parts))))
        }
        "contains"    => { let p = str_arg(&args, 0, "contains")?; Ok(Value::Bool(s.contains(p.as_str()))) }
        "starts_with" => { let p = str_arg(&args, 0, "starts_with")?; Ok(Value::Bool(s.starts_with(p.as_str()))) }
        "ends_with"   => { let p = str_arg(&args, 0, "ends_with")?; Ok(Value::Bool(s.ends_with(p.as_str()))) }
        "replace"     => {
            let from = str_arg(&args, 0, "replace")?;
            let to   = str_arg(&args, 1, "replace")?;
            Ok(Value::Str(Rc::new(s.replace(from.as_str(), to.as_str()))))
        }
        "sub"         => {
            let start = num_arg(&args, 0, "sub")? as usize;
            let end   = num_arg(&args, 1, "sub")? as usize;
            let chars: Vec<char> = s.chars().collect();
            let slice: String = chars[start.min(chars.len())..end.min(chars.len())].iter().collect();
            Ok(Value::Str(Rc::new(slice)))
        }
        "index_of"    => {
            let p = str_arg(&args, 0, "index_of")?;
            Ok(Value::Number(s.find(p.as_str()).map(|i| i as f64).unwrap_or(-1.0)))
        }
        _ => Err(RuntimeError::new(format!("字符串没有方法 '{method}'"))),
    }
}

// ─────────────────────────────────────────
//  Array 方法实现
// ─────────────────────────────────────────

fn array_method_call(arr: Rc<RefCell<Vec<Value>>>, method: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    match method {
        "push"       => { arr.borrow_mut().push(args.into_iter().next().unwrap_or(Value::Nil)); Ok(Value::Nil) }
        "pop"        => Ok(arr.borrow_mut().pop().unwrap_or(Value::Nil)),
        "shift"      => {
            let mut a = arr.borrow_mut();
            if a.is_empty() { Ok(Value::Nil) } else { Ok(a.remove(0)) }
        }
        "unshift"    => {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            arr.borrow_mut().insert(0, v);
            Ok(Value::Nil)
        }
        "len"        => Ok(Value::Number(arr.borrow().len() as f64)),
        "includes"   => {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            Ok(Value::Bool(arr.borrow().iter().any(|x| values_eq(x, &v))))
        }
        "index_of"   => {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            let idx = arr.borrow().iter().position(|x| values_eq(x, &v));
            Ok(Value::Number(idx.map(|i| i as f64).unwrap_or(-1.0)))
        }
        "reverse"    => {
            let reversed: Vec<Value> = arr.borrow().iter().cloned().rev().collect();
            Ok(Value::Array(Rc::new(RefCell::new(reversed))))
        }
        "join"       => {
            let sep = str_arg(&args, 0, "join").unwrap_or_else(|_| Rc::new(",".into()));
            let parts: Vec<String> = arr.borrow().iter().map(|v| format!("{v}")).collect();
            Ok(Value::Str(Rc::new(parts.join(&sep))))
        }
        "slice"      => {
            let start = num_arg(&args, 0, "slice")? as usize;
            let end   = num_arg(&args, 1, "slice")? as usize;
            let a = arr.borrow();
            let slice = a[start.min(a.len())..end.min(a.len())].to_vec();
            Ok(Value::Array(Rc::new(RefCell::new(slice))))
        }
        "concat"     => {
            let other = match args.into_iter().next() {
                Some(Value::Array(a)) => a.borrow().clone(),
                _ => return Err(RuntimeError::new("concat 需要一个 array 参数")),
            };
            let mut result = arr.borrow().clone();
            result.extend(other);
            Ok(Value::Array(Rc::new(RefCell::new(result))))
        }
        "flat"       => {
            let mut result = Vec::new();
            for v in arr.borrow().iter() {
                match v {
                    Value::Array(inner) => result.extend(inner.borrow().clone()),
                    other => result.push(other.clone()),
                }
            }
            Ok(Value::Array(Rc::new(RefCell::new(result))))
        }
        "sort"       => {
            let mut a = arr.borrow().clone();
            a.sort_by(|x, y| {
                cmp_values(x, y).unwrap_or(0).cmp(&0)
            });
            Ok(Value::Array(Rc::new(RefCell::new(a))))
        }
        // map/filter/reduce/find/find_index/every/some 需要回调，在 Interpreter 层处理
        // 这里返回占位错误，实际调用时由上层处理
        _ => Err(RuntimeError::new(format!("array 没有方法 '{method}'（或需要回调，请通过 Interpreter 调用）"))),
    }
}

// ─────────────────────────────────────────
//  Dict 方法实现
// ─────────────────────────────────────────

fn dict_method_call(dict: Rc<RefCell<IndexMap<ValueKey, Value>>>, method: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
    match method {
        "len"    => Ok(Value::Number(dict.borrow().len() as f64)),
        "has"    => {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            let key = Option::<ValueKey>::from(&v).ok_or_else(|| RuntimeError::new("key 不能为 nil"))?;
            Ok(Value::Bool(dict.borrow().contains_key(&key)))
        }
        "keys"   => {
            let keys: Vec<Value> = dict.borrow().keys()
                .map(|k| Value::Str(Rc::new(k.to_string()))).collect();
            Ok(Value::Array(Rc::new(RefCell::new(keys))))
        }
        "values" => {
            let vals: Vec<Value> = dict.borrow().values().cloned().collect();
            Ok(Value::Array(Rc::new(RefCell::new(vals))))
        }
        "delete" => {
            let v = args.into_iter().next().unwrap_or(Value::Nil);
            let key = Option::<ValueKey>::from(&v).ok_or_else(|| RuntimeError::new("key 不能为 nil"))?;
            dict.borrow_mut().swap_remove(&key);
            Ok(Value::Nil)
        }
        "merge"  => {
            let other = match args.into_iter().next() {
                Some(Value::Dict(d)) => d.borrow().clone(),
                _ => return Err(RuntimeError::new("merge 需要一个 dict 参数")),
            };
            let mut result = dict.borrow().clone();
            result.extend(other);
            Ok(Value::Dict(Rc::new(RefCell::new(result))))
        }
        _ => Err(RuntimeError::new(format!("dict 没有方法 '{method}'"))),
    }
}

// ─────────────────────────────────────────
//  参数辅助
// ─────────────────────────────────────────

fn str_arg(args: &[Value], i: usize, method: &str) -> Result<Rc<String>, RuntimeError> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.clone()),
        _ => Err(RuntimeError::new(format!("{method} 第 {i} 个参数必须是 string"))),
    }
}

fn num_arg(args: &[Value], i: usize, method: &str) -> Result<f64, RuntimeError> {
    match args.get(i) {
        Some(Value::Number(n)) => Ok(*n),
        _ => Err(RuntimeError::new(format!("{method} 第 {i} 个参数必须是 number"))),
    }
}
