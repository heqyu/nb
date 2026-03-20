# NB 语言设计文档

> NB（牛逼）—— 一门类 Lua 的现代脚本语言，轻量、嵌入式友好，但引入现代语法和工程特性。
> 实现方式：**Rust 实现，树遍历解释器（AST Interpreter）**

---

## 1. 类型系统

### 基础类型
| 类型 | 说明 |
|------|------|
| `nil` | 空值 |
| `bool` | 布尔值 |
| `number` | 数字（f64） |
| `string` | 字符串 |
| `function` | 函数 |
| `array` | 数组，`[]` 字面量 |
| `dict` | 字典，`{}` 字面量 |
| `class` | 类对象 |
| `mixin` | mixin 对象 |
| `range` | 范围（仅用于 for 迭代） |

### 类型注解
- 风格：后置冒号
- 目的：**服务于工具提示（IDE），运行时完全忽略**
- 无注解变量：编译期类型推断，推断不出为 `any`
- 注解中可以嵌套，但**没有泛型机制**

```nb
let x: number = 10
let name: string = "hello"
let arr: array<number> = [1, 2, 3]
let map: dict<string, number> = { a = 1, b = 2 }
let nested: dict<string, array<number>> = { a = [1,2,3] }
```

### 集合规则
- `array` 用 `[]` 字面量，下标**从 0 开始**
- `dict` 用 `{}` 字面量，key-value 之间用 `=` 赋值
- dict key 支持 string、bool、number（number 会取整存储）
- dict 字面量中，标识符 key 自动转为字符串：`{ name = 1 }` 中的 `name` 即字符串 `"name"`

---

## 2. 变量与可变性（Mutability）

### 设计目标
NB 引入 `mut` 的目的不是实现 Rust 式所有权系统，而是：

> **显式标记"哪些变量可能被修改"，从而控制副作用的可见性**

---

### 变量声明

- `let`：声明不可变变量（默认）
- `let mut`：声明可变变量
- 不可变变量不可被 `=` 重新赋值
- `let` 可以遮蔽（shadowing）同名变量，产生新绑定

```nb
let x = 10              // 不可变
let mut x = 10          // 可变

let arr = [1,2,3]
arr[0] = 10         // ❌
arr.push(4)         // ❌

let mut arr2 = [1,2,3]
arr2[0] = 10             // OK
arr2.push(4)        // ✅

// 模块级变量（文件顶层），所有函数可访问
let MAX = 100
```

> **注意：** 当前实现中，不可变检查仅限于变量重新赋值（`=`）。`arr[0] = 10` 依赖 arr 是否声明为 `mut`，但深层不可变（let arr 的元素禁止修改）暂未在运行时强制检查。

### 写操作定义

以下行为被视为"写操作"，需要目标为 `mut` 变量：

1. 变量重新赋值：`x = expr`
2. 数组元素赋值：`arr[i] = expr`
3. 对象字段赋值：`obj.field = expr`
4. 调用带 `mut` 参数的函数（该参数被传入时）

这些规则在编译期检查。

---

## 3. 注释

```nb
// 单行注释

/// 文档注释（词法层等同单行注释）

/* 多行注释
   可以跨越多行 */
```

> 注意：不使用 Lua 风格的 `--` 注释，使用 C/JS 风格的 `//` 和 `/* */`。

---

## 4. 函数

### 基本语法
```nb
fn add(a: number, b: number): number {
    return a + b
}
```

### 参数可变性
- 默认参数不可变
- `mut` 修饰参数表示函数内可修改该参数
- 编译器会在调用点检查：传入的实参必须为 `mut` 变量
- `mut` 不会在函数调用链中传播
- `mut` 属于变量绑定，而非值本身
- `mut` 仅用于控制"写权限"
- 不引入所有权、借用或生命周期

不进行调用链传播
```nb
fn push(mut arr: array<number>, val: number) {
    arr.push(val)
}

let mut myArr = [1, 2, 3]
push(myArr, 10)     // 调用时无需显式 mut
```

### 多返回值
```nb
fn get_user(id: number): string, number {
    return "rex", 18
}

let name, age = get_user(1)
```

> 多返回值通过 `let a, b = expr` 语法展开，`protect` 块也使用此机制。

### 匿名函数
```nb
let add = fn(a: number, b: number): number {
    return a + b
}
```

### async 函数
```nb
async fn fetch(url: string): string throws {
    // ...
}
```

> **当前状态：** `async`/`await` 词法和语法已实现，运行时直接同步执行（忽略 async 语义），不支持真正的异步并发。

---

## 5. 控制流

### 条件
```nb
if x > 0 {
    print("positive")
} else if x < 0 {
    print("negative")
} else {
    print("zero")
}
```

### 循环
```nb
// range for（不含尾）
for i in 0..5 {         // 0,1,2,3,4
    print(i)
}

// 迭代 array（i 为下标，v 为值）
for i, v in arr {
    print(i, v)
}

// 迭代 dict（k 为 key 字符串，v 为 value）
for k, v in dict {
    print(k, v)
}

// 迭代 string（i 为下标，v 为单字符字符串）
for i, v in "hello" {
    print(i, v)
}

// while
while x > 0 {
    x -= 1
}
```

### 循环控制
```nb
break       // 跳出循环
continue    // 跳过本次迭代
```

---

## 6. 操作符

| 操作符 | 说明 |
|--------|------|
| `+` | 加法（number）或字符串拼接（string + string） |
| `-` `-=` | 减法 |
| `*` `*=` | 乘法 |
| `/` `/=` | 除法（除数为零报运行时错误） |
| `%` | 取模 |
| `+=` | 复合加法赋值 |
| `!=` | 不等于 |
| `==` | 等于 |
| `<` `<=` `>` `>=` | 比较（支持 number 和 string） |
| `&&` | 逻辑与（短路） |
| `\|\|` | 逻辑或（短路） |
| `!` | 逻辑非 |
| `++` `--` | 自增自减（后置语句） |
| `+=` `-=` `*=` `/=` | 复合赋值 |
| `?:` | 三元表达式 |
| `..` | range（`0..5` 表示 0 到 4） |
| `is` | 类型检查 |

```nb
let x = condition ? "yes" : "no"
let s = "hello" + " world"      // 字符串拼接也支持 + 运算符
```

---

## 7. 字符串

### 字符串插值
```nb
let name = "world"
let s = "hello ${name}!"
```

支持在 `${}` 内嵌套任意表达式：
```nb
let s = "result: ${a + b}"
let s = "type: ${type(x)}"
```

转义字符：`\n` `\t` `\\` `\"` `\$`

### 格式化（标准库）
```nb
let s = string.format("hello {0}, you are {1} years old", name, age)
let s = string.format("{0} + {1} = {2}", a, b, a + b)
```

### 字符串拼接
- 支持 `+` 运算符拼接两个字符串
- 也可使用插值或 `string.format`

---

## 8. Class 系统

### 设计原则
- `class` 体内**只声明字段**，不包含方法
- 方法定义在类外部，以 `fn ClassName.method(self, ...)` 的形式绑定
- 实例通过**结构体字面量**创建，未指定字段默认为 `nil`
- 无构造函数（`ctor`）、无 `static`、无 `new` 关键字

### 基本语法
```nb
class Animal {
    name: string        // 不可变字段
    mut hp: number      // 可变字段
}

// 方法定义在类外
fn Animal.speak(self): string {
    return "${self.name} speaks"
}

fn Animal.take_damage(mut self, val: number) {
    self.hp -= val
}

// 结构体字面量初始化（未指定字段默认 nil）
let mut a = Animal { name = "cat", hp = 100 }
a.take_damage(10)
a.speak()
```

### 字段规则
- 字段在 class 体中预声明，类型注解可选
- `mut` 修饰的字段可被方法或外部代码修改
- 未在字面量中指定的字段自动初始化为 `nil`

### 方法规则
- `fn ClassName.method(self, ...)` 语法将函数绑定到类
- `self` 显式声明（第一个参数）
- `mut self` 表示该方法会修改实例，调用时实例必须为 `mut` 变量
- 方法定义顺序不限，可在类定义之后任意位置

### Mixin（混入）
- 用 `mixin` 关键字定义，提供可复用的方法集合
- `require` 声明依赖字段（文档性质，运行时不强制）
- class 通过 `: MixinName` 继承 mixin 的方法
- 支持多 mixin：`class Player : Mixin1, Mixin2`
- 同名方法：class 自身方法优先，其次按 mixin 列表顺序

```nb
mixin Damageable {
    require hp: number

    fn damage(mut self, val) {
        self.hp = self.hp - val
    }

    fn is_dead(self) {
        return self.hp <= 0
    }
}

class Player : Damageable {
    name: string
    mut hp: number
    mut level: number
}

fn Player.level_up(mut self) {
    self.level = self.level + 1
}

let mut p = Player { name = "rex", hp = 100, level = 1 }
p.level_up()
p.damage(30)
p.is_dead()
p is Player      // true
p is Damageable  // true
```

### `to_string` 方法
- class 可定义 `fn ClassName.to_string(self): string` 方法
- `string()` 内置函数会调用它
- **当前实现**：`print` 直接输出 `<类名>` 形式；`string(obj)` 调用 `format!("{v}")`，均输出 `<类名>`；若需自定义字符串表示，通过 `string(obj)` 调用 `to_string` 方法

---

## 9. Mixin 系统

- 用 `mixin` 关键字定义
- mixin **不能直接实例化**
- `require` 声明依赖字段（仅作为文档声明，运行时不强制检查）
- mixin 中的方法会被混入 class

```nb
mixin Damageable {
    require hp: number

    fn damage(mut self, val) {
        self.hp = self.hp - val
    }

    fn is_dead(self) {
        return self.hp <= 0
    }
}

class Player : Entity, Damageable {
    mut level: number
}

fn Player.ctor(mut self, name, hp) {
    self.level = 1
}

let mut p = Player { name = "rex", hp = 100, level = 1 }
p.damage(30)
p.is_dead()
p is Damageable     // true
```

---

## 10. 错误处理

### protect 块
- 捕获**一切运行时错误**，包括 throw、nil 索引、除零等
- 返回多值数组展开：`err` 优先（err first）
- 支持多返回值

```nb
// 无返回值
let err = protect {
    save_to_db(data)
}

// 单返回值
let err, result = protect {
    return parse(input)
}

// 多返回值
let err, name, age = protect {
    return get_user(1)
}

// 忽略错误
protect {
    risky_operation()
}
```

### throw
- 可以 throw 任意值（字符串、数字、对象等）

```nb
throw "something went wrong"
throw NetworkError { code = 408, msg = "timeout" }
```

### ? 错误传播
- 语法已实现（`expr?`），当前运行时直接求值，未实现真正的错误传播语义

```nb
fn load(path: string): string throws {
    let f = open_file(path)?
    return f.read()?
}
```

> **当前状态：** `?` 符号解析正常，运行时等价于直接求值，不会自动向上传播错误。

---

## 11. 异步

### async/await
- `async` 标记异步函数，`await` 在 async 函数内使用
- 词法和语法完整支持

```nb
async fn fetch(url: string): string throws {
    let resp = await http.get(url)?
    return body
}
```

> **当前状态：** 运行时直接同步执行，忽略 async/await 语义，不报错但无异步效果。

---

## 12. 模块系统

### 设计原则
- **文件即模块**：每个 `.nb` 文件是一个模块，模块名为文件名去掉扩展名
- **显式导出**：非 main 文件必须用 `export { ... }` 声明对外暴露的名字；main 文件省略 export
- **显式导入**：用 `require` 语句加载其他模块，支持路径和标准库两种形式
- **无副作用导入**：`require` 只引入被显式 export 的名字，不会执行模块顶层语句的副作用（除非明确设计）

### export

```nb
// math.nb
fn add(a: number, b: number): number {
    return a + b
}

fn _internal() { ... }   // 不在 export 列表，外部不可见

let PI = 3.14159

export { add, PI }
```

- `export` 写在文件末尾（惯例，非强制）
- 只有出现在 `export { ... }` 中的名字才能被外部 `require`
- 未写 `export` 的文件（如 main.nb）表示无需对外暴露

### require

```nb
// 相对路径导入（相对于当前文件）
let math = require("./math")
let result = math.add(1, 2)
print(math.PI)

// 解构导入
let { add, PI } = require("./math")
print(add(1, 2))

// 标准库导入（以 @ 开头）
let fs   = require("@std.fs")
let json = require("@std.json")
```

- `require` 是表达式，返回一个 dict，key 为 export 的名字
- 解构导入是语法糖：`let { a, b } = require("./mod")`
- 标准库路径以 `@` 开头，不带 `.nb` 后缀
- 循环依赖：检测到循环依赖时报运行时错误

### 路径规则
- `"./foo"` 或 `"./foo.nb"` → 相对于当前文件的 `foo.nb`
- `"../utils"` → 上级目录
- `"@std.math"` → 内置标准库模块
- 不支持裸名导入（如 `require("lodash")`），没有包管理器

### 示例

```nb
// utils/math.nb
fn clamp(val, lo, hi) {
    return val < lo ? lo : (val > hi ? hi : val)
}

let TAU = 6.28318

export { clamp, TAU }
```

```nb
// main.nb
let { clamp, TAU } = require("./utils/math")

print(clamp(15, 0, 10))   // 10
print(TAU)                // 6.28318
```

> **当前状态：** `export` 语句解析正常，运行时跳过。`require` 尚未实现。

---

## 13. 闭包

### 捕获语义
- 闭包捕获**创建时的绑定本身**，而非变量名
- `let` 遮蔽产生新绑定，**不影响**已有闭包（创建新子作用域）
- `mut` 修改的是同一绑定，**影响**所有共享该绑定的闭包
- 闭包可以修改捕获的 `mut` 变量

```nb
// 捕获引用
let mut y = 10
let g = fn() { print(y) }
y = 20
g()         // 打印 20

// 闭包内修改捕获的 mut 变量
let mut count = 0
let inc = fn() { count = count + 1 }
inc()
inc()
print(count)    // 2

// 遮蔽：新绑定，不影响已有闭包
let x = 10
let f = fn() { return x }  // 捕获 x=10 这个绑定
let x = 20                  // 新绑定，f 不受影响
f()     // 返回 10
```

### 变量遮蔽（Shadowing）
- `let` 可以遮蔽同名变量，产生新绑定（在当前层存在同名变量时自动创建子作用域）
- 遮蔽可以改变类型

```nb
let a = 1
let a = 2           // OK，遮蔽，新绑定
print(a)            // 2

let a = "hello"     // OK，类型也可以变
print(a)            // "hello"
```

---

## 14. 内置函数

| 函数 | 说明 |
|------|------|
| `print(...)` | 打印输出，多个参数用 Tab 分隔 |
| `type(x)` | 返回类型字符串，见下方规则 |
| `string(x)` | 转换为字符串 |
| `tonumber(x)` | 转换为数字，失败返回 `nil` |
| `len(x)` | 返回 array/string/dict 的长度 |
| `assert(x, msg)` | 断言，失败则 throw msg |

### type() 返回规则
```nb
type(nil)               // "nil"
type(true)              // "bool"
type(42)                // "number"
type("hello")           // "string"
type(fn(){})            // "function"
type([1,2,3])           // "array"
type({a=1})             // "dict"
type(Animal)            // "class"
type(Damageable)        // "mixin"
type(Animal { hp = 1 }) // "模块名.Animal"   全限定名（模块名取自文件名去扩展名）
```

### string() / print()
- `string(x)` 输出与 `print` 相同的格式
- 数字输出：整数部分无小数点（`42` 而非 `42.0`），大数或有小数则正常浮点
- Instance 输出为 `<类名>`（当前未调用 `to_string` 方法）

### string 模块
```nb
string.format("{0} + {1} = {2}", a, b, a + b)
```
`string` 作为全局 dict 对象提供，通过 `.` 访问其方法。

---

## 15. 字符串方法

字符串采用方法调用风格：

```nb
let s = "hello world"

s.len()                     // 长度（按 Unicode 字符计）
s.upper()                   // 大写
s.lower()                   // 小写
s.trim()                    // 去首尾空白
s.split(" ")                // 分割，返回 array
s.contains("hello")         // 是否包含，返回 bool
s.starts_with("he")         // 是否以此开头，返回 bool
s.ends_with("ld")           // 是否以此结尾，返回 bool
s.replace("hello", "hi")    // 替换，返回新字符串
s.sub(0, 5)                 // 切片（按字符下标），返回新字符串
s.index_of("world")         // 查找位置，找不到返回 -1

// 格式化
string.format("{0} + {1} = {2}", a, b, a + b)
```

---

## 16. 标准库

| 模块/对象 | 说明 | 状态 |
|-----------|------|------|
| `string`（全局 dict） | 字符串工具（`string.format`） | ✅ 已实现 |
| `print` / `type` / `len` / `assert` / `string()` / `tonumber()` | 全局内置函数 | ✅ 已实现 |
| 字符串方法 | `.len()` `.upper()` `.lower()` `.trim()` `.split()` 等 | ✅ 已实现 |
| array 方法 | 见附录A | ✅ 已实现 |
| dict 方法 | 见附录B | ✅ 已实现 |
| `@std.*` 模块（fs/io/math/os 等） | 需通过 require 加载 | ⏳ 待实现 |

---

## 17. 实现方式

| 项目 | 决策 |
|------|------|
| 实现语言 | **Rust** |
| 执行方式 | **树遍历解释器（AST Interpreter）** |
| 编译目标 | 直接解释 AST，暂无字节码编译 |
| 入口命令 | `nb run <文件.nb>` |
| 模块名 | 取自文件名（去掉扩展名） |

### 已实现特性
- ✅ 词法分析（Lexer）：所有关键字、操作符、字符串插值
- ✅ 语法分析（Parser）：完整 AST
- ✅ 变量绑定、遮蔽、可变性
- ✅ 函数定义与调用（含匿名函数、闭包）
- ✅ 多返回值 / `let a, b = expr`
- ✅ 控制流：if/else、for/while、break/continue
- ✅ for 迭代：range、array、dict、string
- ✅ Class 定义（纯字段）+ Mixin 混入
- ✅ 外部方法绑定：`fn ClassName.method(self, ...)`
- ✅ 结构体字面量初始化：`ClassName { field = val, ... }`
- ✅ `is` 类型检查（含 mixin 链）
- ✅ protect 错误捕获
- ✅ throw 抛出错误
- ✅ 字符串插值（`${expr}`）
- ✅ array / dict 字面量及内置方法
- ✅ `string.format`
- ✅ 内置函数（print / type / len / assert / tonumber / string）

### 待实现 / 已知限制
- ⏳ 字节码编译（当前为树遍历解释）
- ⏳ 模块系统（`require` / `export`）
- ⏳ 真正的 async/await（当前同步执行）
- ⏳ `?` 错误传播（当前等价于直接求值）
- ⏳ 深层不可变（`let arr` 元素禁止修改）
- ⏳ 标准库（`@std.fs` / `@std.io` / `@std.math` 等）
- ⏳ `string(obj)` 自动调用 `to_string` 方法

---

## 附录A：Array 内置方法

```nb
let mut arr = [1, 2, 3, 4, 5]

// 增删
arr.push(6)             // 末尾添加
arr.pop()               // 末尾移除并返回
arr.shift()             // 头部移除并返回
arr.unshift(0)          // 头部添加
// arr.splice(1, 2)     // ⏳ 待实现

// 查找
arr.index_of(3)         // 查找值，返回下标，找不到返回 -1
arr.includes(3)         // 是否包含，返回 bool
arr.find(fn(x) { return x > 2 })       // 返回第一个满足条件的元素，没有返回 nil
arr.find_index(fn(x) { return x > 2 }) // 返回第一个满足条件的下标，没有返回 -1

// 变换（返回新数组，不修改原数组）
arr.map(fn(x) { return x * 2 })
arr.filter(fn(x) { return x > 2 })
arr.reduce(fn(acc, x) { return acc + x }, 0)
arr.slice(1, 3)         // 切片，返回新数组
arr.reverse()           // 反转，返回新数组
arr.concat([6, 7, 8])   // 拼接，返回新数组
arr.flat()              // 展平一层
arr.join(", ")          // 转字符串

// 排序（返回新数组，不修改原数组）
arr.sort()                              // 默认升序（number 或 string）
arr.sort(fn(a, b) { return a - b })     // 自定义比较（返回负数/0/正数）

// 判断
arr.every(fn(x) { return x > 0 })  // 全部满足
arr.some(fn(x) { return x > 3 })   // 至少一个满足

// 长度
arr.len()               // 返回长度
```

**方法链式调用：**
```nb
let result = data
    .filter(fn(x) { return x % 2 == 0 })
    .sort(fn(a, b) { return b - a })
    .map(fn(x) { return x * 10 })
```

## 附录B：Dict 内置方法

```nb
let mut d = { name = "NB", version = 1 }

// 查询
d.has("name")           // 是否有某个 key，返回 bool
d.keys()                // 返回所有 key 的 array（key 均转为字符串）
d.values()              // 返回所有 value 的 array
d.len()                 // 返回键值对数量

// 修改
d.delete("version")     // 删除某个 key
d.merge({ extra = 4 })  // 合并另一个 dict，返回新 dict（不修改原 dict）
```

---

## 附录C：完整示例

```nb
// advanced.nb

mixin Damageable {
    require hp: number

    fn damage(mut self, val) {
        self.hp = self.hp - val
    }

    fn is_dead(self) {
        return self.hp <= 0
    }
}

// class 只声明字段
class Player : Damageable {
    name: string
    mut hp: number
    mut level: number
}

// 方法定义在类外
fn Player.level_up(mut self) {
    self.level = self.level + 1
}

fn Player.to_string(self) {
    return "${self.name}(hp=${self.hp}, lv=${self.level})"
}

// 结构体字面量初始化
let mut p = Player { name = "rex", hp = 100, level = 1 }
p.level_up()
p.damage(30)
print(p.hp)         // 70
print(p.is_dead())  // false
print(p is Player)  // true
print(p is Damageable)  // true

// array 方法链
let data = [5, 3, 8, 1, 9, 2, 7, 4, 6]
let result = data
    .filter(fn(x) { return x % 2 == 0 })
    .sort(fn(a, b) { return b - a })
    .map(fn(x) { return x * 10 })
print(result.join(", "))        // 80, 40, 20

// protect 错误处理
let err, result = protect {
    return 10 + 20
}
if err != nil {
    print("error: ${err}")
} else {
    print("result: ${result}")  // result: 30
}

let err2 = protect {
    throw "oops"
}
print("caught: ${err2}")        // caught: oops

// 闭包与遮蔽
let threshold = 5
let big = [1, 8, 3, 9, 2, 7, 4, 6]
let filtered = big.filter(fn(x) { return x > threshold })
print(filtered.join(", "))      // 8, 9, 7, 6

let x = 10
let f = fn() { return x }
let x = 20          // 遮蔽
print(f())          // 10（捕获的是创建时的绑定）

// string 方法
let s = "  hello world  "
print(s.trim().upper())                   // HELLO WORLD
print(s.trim().split(" ").join("-"))      // hello-world

// string.format
print(string.format("Hello {0}, v{1}", "NB", 1))  // Hello NB, v1
```
