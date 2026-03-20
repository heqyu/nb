/// 插值字符串的组成部分（在 Parser 阶段已将表达式解析为 AST）
#[derive(Debug, Clone)]
pub enum InterpPart {
    Literal(String),
    Expr(Box<Expr>),
}

/// 源码位置（行列号，1-based）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

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
        span: Span,
    },
    /// let x = expr  /  let mut x = expr
    Let {
        name: String,
        mutable: bool,
        type_ann: Option<TypeAnnotation>,
        value: Option<Expr>,
        span: Span,     // let 关键字位置
        name_span: Span, // 变量名位置
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
        span: Span,
    },
    /// for i in expr { body }
    ForIn {
        key: String,
        value: Option<String>,
        value_mutable: bool,
        iter: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    /// while cond { body }
    While {
        cond: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
    Break,
    Continue,
    /// class 定义
    ClassDef(ClassDef),
    /// mixin 定义
    MixinDef(MixinDef),
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
    pub name_span: Span,        // 函数名位置（匿名函数为 fn 关键字位置）
    pub receiver: Option<String>, // fn Player.method 中的 "Player"
    pub async_: bool,
    pub params: Vec<Param>,
    pub ret_type: Option<TypeAnnotation>,
    pub throws: bool,
    pub body: Vec<Stmt>,
    pub span: Span,             // fn 关键字位置
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub name_span: Span,        // 参数名位置
    pub mutable: bool,
    pub type_ann: Option<TypeAnnotation>,
}

/// class 定义（只含字段，方法通过 fn ClassName.method 定义）
#[derive(Debug, Clone)]
pub struct ClassDef {
    pub name: String,
    pub name_span: Span,        // 类名位置
    pub mixins: Vec<String>,    // 混入的 mixin 列表
    pub fields: Vec<FieldDef>,
    pub span: Span,             // class 关键字位置
}

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub name_span: Span,        // 字段名位置
    pub mutable: bool,
    pub type_ann: Option<TypeAnnotation>,
}


/// mixin 定义
#[derive(Debug, Clone)]
pub struct MixinDef {
    pub name: String,
    pub name_span: Span,        // mixin 名位置
    pub requires: Vec<FieldDef>,
    pub methods: Vec<FnDef>,
    pub span: Span,             // mixin 关键字位置
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
    InterpolatedString(Vec<InterpPart>),
    /// 标识符，携带位置信息
    Ident(String, Span),

    /// 二元运算
    BinOp { left: Box<Expr>, op: BinOp, right: Box<Expr> },
    /// 一元运算  !x  -x
    UnaryOp { op: UnaryOp, expr: Box<Expr> },
    /// 三元  cond ? a : b
    Ternary { cond: Box<Expr>, then: Box<Expr>, else_: Box<Expr> },

    /// 函数调用  f(args)
    Call { callee: Box<Expr>, args: Vec<CallArg>, span: Span },
    /// 成员访问  obj.field
    Field { obj: Box<Expr>, field: String, field_span: Span },
    /// 下标  arr[idx]
    Index { obj: Box<Expr>, idx: Box<Expr> },

    /// new → 结构体字面量  ClassName { field = val, .. }
    StructLit { class: String, class_span: Span, fields: Vec<(String, Span, Expr)> },
    /// obj is Type
    Is { expr: Box<Expr>, type_name: String, type_span: Span },

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

impl Expr {
    /// 获取表达式的 span（用于 LSP 位置计算）
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident(_, s)              => *s,
            Expr::Call { span, .. }        => *span,
            Expr::Field { field_span, .. } => *field_span,
            Expr::StructLit { class_span, .. } => *class_span,
            _                            => Span::default(),
        }
    }
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
