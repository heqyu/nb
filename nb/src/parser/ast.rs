use crate::lexer::StringPart;

/// 类型注解（仅用于工具提示，不做运行时检查）
#[derive(Debug, Clone, PartialEq)]
pub enum TypeAnnotation {
    Simple(String),                          // number, string, bool ...
    Generic(String, Vec<TypeAnnotation>),    // array<number>, dict<string, number>
    Any,
}

/// 顶层语句
#[derive(Debug, Clone)]
pub enum Stmt {
    /// let a, b, c = expr（多变量绑定，用于 protect 返回值）
    MultiLet {
        names: Vec<String>,
        mutable: bool,
        value: Option<Expr>,
    },
    /// let x = expr  /  let mut x = expr
    Let {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnnotation>,
        value: Option<Expr>,
    },
    /// 赋值  x = expr  /  x.field = expr  /  x[idx] = expr
    Assign {
        target: Expr,
        value: Expr,
    },
    /// 复合赋值  x += expr
    CompoundAssign {
        target: Expr,
        op: BinOp,
        value: Expr,
    },
    /// x++  x--
    IncDec {
        target: Expr,
        inc: bool,
    },
    /// fn name(params) : ret { body }
    FnDef(FnDef),
    /// return expr
    Return(Option<Expr>),
    /// if/else if/else
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_ifs: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    /// for i in expr { body }
    ForIn {
        key: String,
        value: Option<String>,
        value_mutable: bool,
        iter: Expr,
        body: Vec<Stmt>,
    },
    /// while cond { body }
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    /// class 定义
    ClassDef(ClassDef),
    /// trait 定义
    TraitDef(TraitDef),
    /// throw expr
    Throw(Expr),
    /// export { a, b, c }
    Export(Vec<String>),
    /// 表达式语句
    Expr(Expr),
}

/// 函数定义
#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: Option<String>,   // None 表示匿名函数
    pub async_: bool,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeAnnotation>,
    pub throws: bool,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub mutable: bool,
    pub type_ann: Option<TypeAnnotation>,
}

/// class 定义
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub parents: Vec<String>,   // 继承和 trait
    pub fields: Vec<FieldDef>,
    pub methods: Vec<MethodDef>,
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub mutable: bool,
    pub type_ann: Option<TypeAnnotation>,
}

#[derive(Debug, Clone)]
pub struct MethodDef {
    pub static_: bool,
    pub fn_def: FnDef,
}

/// trait 定义
#[derive(Debug, Clone)]
pub struct TraitDef {
    pub name: String,
    pub requires: Vec<FieldDef>,
    pub methods: Vec<FnDef>,
}

/// 二元操作符
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, NotEq, Lt, LtEq, Gt, GtEq,
    And, Or,
    Range,  // ..
}

/// 表达式
#[derive(Debug, Clone)]
pub enum Expr {
    Nil,
    Bool(bool),
    Number(f64),
    StringLit(String),
    InterpolatedString(Vec<StringPart>),
    Ident(String),

    /// 二元运算
    BinOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    /// 一元运算  !x  -x
    UnaryOp { op: UnaryOp, expr: Box<Expr> },
    /// 三元  cond ? a : b
    Ternary { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },

    /// 函数调用  f(args)
    Call { callee: Box<Expr>, args: Vec<CallArg> },
    /// 成员访问  obj.field
    Field { obj: Box<Expr>, field: String },
    /// 下标  arr[idx]
    Index { obj: Box<Expr>, idx: Box<Expr> },

    /// new Class(args)
    New { class: String, args: Vec<CallArg> },
    /// obj is Type
    Is { expr: Box<Expr>, type_name: String },

    /// 匿名函数  fn(params) { body }
    Fn(Box<FnDef>),

    /// protect { body }
    Protect(Vec<Stmt>),
    /// await expr
    Await(Box<Expr>),
    /// expr?  错误传播
    Try(Box<Expr>),

    /// array 字面量  [1, 2, 3]
    Array(Vec<Expr>),
    /// dict 字面量  { a = 1, b = 2 }
    Dict(Vec<(Expr, Expr)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
    Not,
}

/// 函数调用参数（支持 mut 标记）
#[derive(Debug, Clone)]
pub struct CallArg {
    pub mutable: bool,
    pub expr: Expr,
}
