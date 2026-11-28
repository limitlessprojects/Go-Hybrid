# PREREG — issue #4596 wildcard spike (reversible checked regions)

此文件在任何实现代码、任何测量之前写成。写完后**不再修改**（除文末"补记"若有不影响预言含义的笔误）。
RESULTS.md 与它对照填写；任何被推翻的预言原样保留并标注。

## H1 — 精确撤销（round-trip 逐字节）

对全部被接纳的程序 `p`：

```
inverse_p(forward_p(s, key), key) == s     // 64 字节逐字节相等
```

覆盖：每个程序 ≥ 100,000 条 PCG 种子随机记录；整数边界语料（0, 1, ^0, 1<<63, 1<<63-1,
0xAAAA.., 0x5555.., 小模式）；每条含 `if` 的程序必须两条分支路径都被覆盖。

**证伪条件**：任意一个程序的任意一条记录出现 1 个字节不等，H1 即被证伪，且"精确撤销"的
主张整条作废（不是"部分成立"）。

## H2 — 独立实现一致

IR 合成的逆向函数（解释器 + 发射出的 Go 代码）必须与**手写**的独立逆向实现一致：
手写实现不复用 IR、不读 IR 的反向规则，直接按语义写出逆变换（例：`x0 = rotr(x0 ^ key, 7)`
这样的表达式，按语句逆序）。

三路对账：`gen.Inverse`（发射代码）、`ir` 解释器逆向、`refimpl` 手写逆向。

**证伪条件**：三路中任意两路在任意记录上不一致。

## H3 — fail closed 与诊断可区分

10 个负例（`x = 0`、掩码、截断移位、判别子被覆盖、判别子来自非 owned 源、切片别名逃逸、
channel send、间接外部调用、task capture、非可逆原语）必须**全部被拒**，且每个负例的首个
诊断码必须互不相同（10 个不同的具名码）。

**证伪条件**：任一负例被接纳，或两个负例的首个诊断码相同。

## H4 — ROI（1 MiB 状态）

在 1 MiB 状态（131072 × uint64）上比较三种"变换后回滚"方案：

- `snapshot`：变换前整块拷贝，回滚时拷回
- `undolog`：逐步记录 (index, old value)，回滚时逆序回放
- `reversible`：只做变换，回滚 = 执行机械推导的逆函数

用 **Go benchmark**（`testing.B` + `-benchmem`），报 ns/op、B/op、allocs/op、额外峰值字节
（按方案结构解析计算并在文中说明）、正/逆延迟、代码行数。

**H4 成立的条件（预先声明的数字门槛）**：
1. `reversible` 的额外峰值字节 ≤ `snapshot` 的 25%（即 ≤ 256 KiB）；
2. `reversible` 的 forward+rollback 单次总延迟 ≤ `snapshot` 的 2.0 倍；
3. 逆函数代码量 ≤ 正函数的 1.5 倍。

任一不满足 → H4 被证伪，结论写成"内存换时间不划算"。

## H4b — 子集是否窄到没意义（预注册阈值）

用一个 24 条"常见变换片段"语料（ARX 轮、掩码、移位混合、查表 S-box、乘法混合、带标志的
条件赋值、不可逆清零等），**先声明**每条应被接纳还是应被哪个诊断拒绝，再跑检查器比对。

**预注册判定**：若被接纳比例 **< 20%**，则结论为"这个子集太窄，不值得作为语言特性"，
并在 RESULTS 中量化"仅因判别子保留规则被拒"的比例。

## 不做的事（预先声明边界）

- 不实现 Hybrid 前端、不改 `Go-Hybrid` 仓库、不声称上游实现。
- 不撤销任何外部副作用：网络、文件、goroutine、panic 都不在模型内。
- 不处理并发：区域是单线程的。
- 不证明定宽语义之外的东西；所有算术都是 mod 2^64 或 2^8。
- 不联网、不加第三方依赖、只跑本机 go1.27.4。

## 诊断码目录（实现前定稿）

| code | 含义 |
|---|---|
| R001_ASSIGN_CONST | `x = k`：常量赋值，非单射 |
| R002_LOSSY_MASK | `x &= k`（k ≠ ^0）：清零位 |
| R003_LOSSY_SHIFT | `x <<= n` / `x >>= n`（n ≠ 0）：丢位 |
| R004_DISCRIMINATOR_CLOBBERED | 分支内写入了判别子所在寄存器，判别子无法从出口状态重建 |
| R005_DISCRIMINATOR_FOREIGN | 判别子来自非 owned 值（外部/未保留） |
| R006_ALIAS_ESCAPE | 可变切片/指针别名逃出区域 |
| R007_EFFECT_CHANSEND | 区域内 channel send |
| R008_FOREIGN_CALL | 间接/外部调用，无逆向契约 |
| R009_TASK_CAPTURE | spawn / task capture |
| R010_NONINVERTIBLE_PRIM | 原语退化：`x ^= x`、`x -= x`、rot by 0、非双射 perm、`swap x,x` |
| R011_UNBOUNDED_LOOP | 无界循环：迭代次数不是状态函数 |
| R012_PRIM_NOT_IN_ALGEBRA | 原语不在声明代数内（乘法混合、任意查表） |
| R013_WRITE_IMMUTABLE_KEY | 写 key（只读保留值） |
| R014_SBOX_NOT_BIJECTIVE | 声明的 S-box 表不是双射 / 未声明 |
