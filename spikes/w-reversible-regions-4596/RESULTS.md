# RESULTS — issue #4596 wildcard spike: 可逆检查区域

预注册见 [PREREG.md](PREREG.md)（在写任何实现代码之前落盘，之后未改动其预言）。
本文所有数字都是本机跑出来的；没跑的一律标「未验证」。

环境：Windows · amd64 · `go1.27.4` · Intel i5-1035G4 @1.10GHz（8 逻辑核）·
只用标准库、不联网。

---

## 0. 一句话结论

**精确撤销在值状态上成立且便宜**（10 个程序、1,011,680 条记录、逐字节 0 失败；1 MiB 上
比 snapshot 少 1 MiB 且总延迟 0.74×），**但这个子集的瓶颈是原语代数，不是分支规则**：
24 条真实形态变换只接纳 7 条（29.2%），被拒的 17 条里只有 1 条是被分支规则拒的，却有 3 条
（xorshift 两半、奇数乘法）在数学上本来可逆、只是不在我声明的逐语句代数里。

---

## 1. 预先声明的假设与判定

数字门槛在 PREREG 里写死，此处只填结果。

| # | 假设 | 证伪条件 | 实测 | 判定 |
|---|---|---|---|---|
| **H1** | 对每个被接纳程序 `inverse(forward(s)) == s`（64 字节逐字节） | 任意 1 字节不等即整条作废 | 1,011,680 条记录，**0 条失败**（含整数边界、两条以上分支路径） | **成立** |
| **H2** | 三路实现一致：发射的 Go 代码 / `ir` 解释器 / 手写 `refimpl` | 任意两路在任意记录上不一致 | 1,011,680 条记录 × 3 路，**0 处不一致**；正逆双向都比 | **成立** |
| **H3** | 10 个负例全部 fail closed 且诊断码互不相同 | 任一被接纳，或两例共享诊断码 | 10/10 拒绝，**10 个不同码**；14/14 目录条目都被触发过 | **成立** |
| **H4** | 1 MiB 上逆向执行的回滚开销可接受（3 个门槛） | 任一门槛不满足 | 额外字节 **0 B**（门槛 ≤256 KiB）；总延迟 **0.32–0.74×** snapshot（门槛 ≤2.0×，三批都满足）；逆/正代码体积 **1.01×**（门槛 ≤1.5×） | **成立** |
| **H4b** | 子集窄到没意义？ | 语料接纳率 < 20% | **29.2%（7/24）** | **未证伪**，但见 §6 的更细归因 |

H4b 的阈值是 20%，实测 29.2% 压线通过；可是按拒绝原因拆开看（§6），真正卡住可逆程序的是
代数而不是分支规则，这条负面结论比"29.2% > 20%"更重要。

---

## 2. 原语代数、定宽语义、分支与别名规则

**状态**：8 个 owned 64 位寄存器 `x0..x7` + 1 个只读保留字 `key`（索引 8）。
`key` 不参与状态等价比较，但正逆函数都要收到它，所以"保留值"是可逆的。

**定宽语义**：加法/减法一律 mod 2^64；S-box 逐字节 mod 2^8；旋转量取 `n mod 64`。
没有符号、没有溢出陷阱、没有浮点。

**允许的原语与其逆（这就是"原语代数"，机械可查）**

| 原语 | 语义 | 逆 |
|---|---|---|
| `add r,k` / `sub r,k` | `r ±= k` | `sub` / `add` 同 k |
| `addv r,s` / `subv r,s` | `r ±= s`（要求 `r≠s`） | 交换 `addv`↔`subv`（依赖整段逆序） |
| `xor r,s` / `xori r,k` | `r ^= s` / `r ^= k` | 自身 |
| `rotl r,n` / `rotr r,n` | 循环移位，`n≠0 mod 64` | `rotr` / `rotl` 同 n |
| `swap a,b` | 交换，`a≠b` | 自身 |
| `perm p0..p7` | `new[i]=old[p[i]]`，p 必须双射 | `q[p[i]]=i` 的置换 |
| `sboxb r,T` | 逐字节代换，T 必须是**声明过且验证为双射**的表 | 表求逆得到的 `T⁻¹` |
| `if bit r,b == v` … | 条件：判别子必须在出口状态里存活 | 同一个 `if`，两臂各取逆、臂内反序 |

**分支判别子规则（这是本 spike 的核心约束）**
判据：`if bit r,b == v` 的寄存器 `r` **不得被任何一条臂（含嵌套）写入**。
于是 `r` 的出口值 == 入口值，逆函数在"撤销到这个 if 之前"重新判定，必然选到同一条臂。
判别子来自 `key` 是允许的（`key` 只读）。判别子来自非 owned 值一律拒（R005）——那等于偷偷
长出一个日志，正是 issue 排除的做法。

**别名/效应限制**：区域状态只允许寄存器；可变切片/指针别名逃逸（R006）、channel send（R007）、
无逆向契约的外部调用（R008）、task capture（R009）、无界循环（R011）全部 fail closed。

**诊断码目录（14 个，全部被真实语料触发过）**

`R001_ASSIGN_CONST` · `R002_LOSSY_MASK` · `R003_LOSSY_SHIFT` · `R004_DISCRIMINATOR_CLOBBERED` ·
`R005_DISCRIMINATOR_FOREIGN` · `R006_ALIAS_ESCAPE` · `R007_EFFECT_CHANSEND` · `R008_FOREIGN_CALL` ·
`R009_TASK_CAPTURE` · `R010_NONINVERTIBLE_PRIM` · `R011_UNBOUNDED_LOOP` · `R012_PRIM_NOT_IN_ALGEBRA` ·
`R013_WRITE_IMMUTABLE_KEY` · `R014_SBOX_NOT_BIJECTIVE`

---

## 3. 20 个例子：10 正 / 10 负

判定由 `go run ./cmd/revsynth` 与 `go test ./harness -run TestAdmission` 双向核对，44/44 条
预注册判定全部成立（`out/revsynth.txt` 里 `ok=true` 列）。

### 10 个正例（全部被接纳，且发射出的 Go 参与 H1/H2 测试）

| # | 案例 | 内容 | 逆向要点 |
|---|---|---|---|
| 1 | `pos_rotl_xor_key` | `rotl x0,7; xori x0,key` | issue 原文最小样例：`x0 = rotr(x0^key,7)` |
| 2 | `pos_swap_pair` | 两对 swap + 常量 XOR | 抓"只逐条取逆、忘记反序"的 bug |
| 3 | `pos_arx_round` | ARX 轮（add-rotate-xor）+ sub | `addv/subv` 的逆序依赖 |
| 4 | `pos_perm_cycle` | 双射置换 + 两条普通原语 | 逆置换 `q[p[i]]=i`；发射成轮换，无自赋值 |
| 5 | `pos_branch_addsub` | 判别子 `x7.bit0`，两臂各写一个寄存器 | 出口 `x7` 不变 ⇒ 逆可重判 |
| 6 | `pos_branch_nested` | 嵌套 if，`x6` 同时是两个判别子 | 两层判别子都存活 |
| 7 | `pos_branch_xor_key` | 判别子 = `key.bit4` | 保留值分支（常量分支） |
| 8 | `pos_wide_mix` | 覆盖 `x0..x7` 与全部原语族，无分支 | 逐条反序 |
| 9 | `pos_sbox_bytes` | 声明仿射表 `S1`，逐字节代换 | 表求逆得 `S1⁻¹`，不信任声明 |
| 10 | `pos_full_mix_branch` | perm + S-box + 立即数 + 分支 | 组合压力：臂对称撤销 → 反序 |

### 10 个负例（全部 fail closed，诊断码互不相同）

| # | 案例 | 触发的构造 | 首个诊断码 | 行 |
|---|---|---|---|---|
| 1 | `neg_assign_zero` | `set x0,0`（前一行 `add x0,5` 合法） | `R001_ASSIGN_CONST` | 8 |
| 2 | `neg_lossy_mask` | `mask x1,0x7f`（清 57 位） | `R002_LOSSY_MASK` | 7 |
| 3 | `neg_shift_trunc` | `shr x2,8` | `R003_LOSSY_SHIFT` | 7 |
| 4 | `neg_disc_clobber` | 判别子 `x7.bit0`，else 臂 `xori x7,0x10` | `R004_DISCRIMINATOR_CLOBBERED` | 8 |
| 5 | `neg_disc_foreign` | `if foreign bit 3 == 1` | `R005_DISCRIMINATOR_FOREIGN` | 8 |
| 6 | `neg_alias_escape` | `alias "sharedBuf[:]"` | `R006_ALIAS_ESCAPE` | 9 |
| 7 | `neg_chansend` | `chansend x1` | `R007_EFFECT_CHANSEND` | 7 |
| 8 | `neg_foreign_call` | `call "peer.Commit"` | `R008_FOREIGN_CALL` | 7 |
| 9 | `neg_task_capture` | `capture "task"` | `R009_TASK_CAPTURE` | 7 |
| 10 | `neg_noninv_prim` | `xor x3,x3`（原语退化成清零） | `R010_NONINVERTIBLE_PRIM` | 9 |

隔离性在这 10 条里以两种方式体现：效应类（6/7/8/9）在**解析后立刻**被拒，永远进不到
发射器；状态类（1/2/3/4/10）在数据流检查时被拒。`gen/` 里不存在任何含 `chansend`/`call`/
`alias`/`capture` 的函数——被接纳集合里没有一条效应语句（`TestAdmission` 断言 10 个正例
`Check()==∅`，`TestEveryDiagnosticIsExercised` 断言目录 14/14 都被触发）。

---

## 4. Round-trip 实测（H1 + H2）

命令：`go test ./harness -v -count=1`。

```
TOTAL: 1011680 records, 1011680 inverse executions, 3-way agreement (emitted / interpreter / hand-written)
PASS  revreg/harness
```

| 项目 | 实测 |
|---|---|
| 每个正例记录数 | **101,168**（100,000 条 PCG 随机 + 1,168 条边界语料） |
| 10 个正例合计 | **1,011,680** 条记录 |
| 每条记录的断言 | 5 项：(a) 手写正向 ≡ 发射正向；(b) 解释器正向 ≡ 发射正向；(c) `Inverse(Forward(s)) == s`；(d) 手写逆向 ≡ 发射逆向且回原值；(e) 解释器跑逆程序回原值 |
| 等价性判据 | 64 字节小端编码逐字节比较（`bytesOf`），不是结构体 `==` |
| **失败数** | **0** |
| 边界语料 | 584 个状态 × 2 个 key（`0` 与 `^0`）= 1,168 条/例；含 `0, 1, ^0, 1<<63, (1<<63)-1, 0xAAAA…, 0x5555…, 0xFF` |
| 分支覆盖率 | 见下表，每个 if 的两条臂都被走到 |

分支路径计数（`else/then`，来自解释器轨迹）：

| 案例 | if#0 | if#1 |
|---|---|---|
| `pos_branch_addsub` | 50,383 / 50,785 | — |
| `pos_branch_nested` | 50,486 / 50,682 | 25,412 / 25,270（仅在外层 then 内可达） |
| `pos_branch_xor_key` | 50,663 / 50,505（判别子 = `key.bit4`） | — |
| `pos_full_mix_branch` | 50,590 / 50,578 | — |

**为什么这个测试值得信**

1. 三路独立：发射器走 IR→Go 文本；解释器走 IR→直接执行；`refimpl` 是手写的、不 import `ir`、
   自带 `S` 类型，S-box 逆用"解 `x·a ≡ 1 (mod 256)` 再乘回"推导，和检查器的表求逆不是同一算法。
2. 测试会**先**断言手写正向与发射正向一致（(a)），否则"两边都错得一样"的 round-trip 会被漏掉。
3. 每次不一致都会 `t.Fatalf` 并打印输入/正向/回滚的十六进制，不会静默通过。

生成的代码规模（发射器实测字节数，见 `out/revsynth.txt`）：

| 案例 | 正向 B/行 | 逆向 B/行 | 比值 |
|---|---|---|---|
| `pos_rotl_xor_key` | 227 / 6 | 228 / 6 | 1.00 |
| `pos_swap_pair` | 241 / 7 | 241 / 7 | 1.00 |
| `pos_arx_round` | 268 / 10 | 269 / 10 | 1.00 |
| `pos_perm_cycle` | 316 / 17 | 316 / 17 | 1.00 |
| `pos_branch_addsub` | 270 / 9 | 270 / 9 | 1.00 |
| `pos_branch_nested` | 401 / 16 | 403 / 16 | 1.00 |
| `pos_branch_xor_key` | 321 / 11 | 322 / 11 | 1.00 |
| `pos_wide_mix` | 357 / 13 | 357 / 13 | 1.00 |
| `pos_sbox_bytes` | 269 / 8 | 276 / 8 | 1.03 |
| `pos_full_mix_branch` | 457 / 23 | 468 / 23 | 1.02 |
| **合计** | **3,127 B / 120 行** | **3,150 B / 120 行** | **1.01** |

逆函数几乎不多花代码——它是同构的语句序列。**但**：表类原语要多一份逆表（每个 256 B，
方向共享常量不计入函数体积），这是这张表**没有**算进去的成本。

---

## 5. ROI：1 MiB 状态上三种"变换后回滚"（H4）

**方式：Go benchmark**（`testing.B` + `-benchmem`，`roi/roi_test.go`），不是 Python/tracemalloc。
工作负载：1 MiB = 131,072 × `uint64`；一次 ARX 扫描
`rotl17 → ^key → +stride*i → rotl41`，然后"失败"并回滚。

命令：`go test ./roi -run TestSchemesRestoreExactly -bench . -benchmem -count=3`
（`TestSchemesRestoreExactly` 先证明三种方案都把缓冲区**逐字节**还原，再谈性能。）

### 第一批（空载）

| 方案 | run1 | run2 | run3 | 中位数 | 额外峰值字节 | allocs/op |
|---|---|---|---|---|---|---|
| `ForwardOnly`（仅变换） | 260,163 | 328,555 | 300,009 | **300,009** | 0 | 0 |
| `InverseOnly`（仅逆执行） | 293,714 | 321,369 | 287,328 | **293,714** | 0 | 0 |
| `SnapshotRollback`（拷贝→变换→拷回） | 936,970 | 1,043,492 | 795,471 | **936,970** | **1,048,581 B ≈ 1 MiB** | 1 |
| `UndoLogRollback`（边变换边记 (idx,old)→逆序回放） | 3,640,764 | 2,028,323 | 3,504,574 | **3,504,574** | **2,097,158 B = 2 MiB** | 1 |
| `ReversibleRollback`（变换→执行逆函数） | 845,850 | 689,871 | 641,955 | **689,871** | **0 B** | **0** |

派生数字（中位数）：

| 指标 | snapshot | undo-log | 逆执行 |
|---|---|---|---|
| forward+rollback 延迟 | 936,970 ns | 3,504,574 ns | **689,871 ns** |
| 相对 snapshot | 1.00× | **3.74×** | **0.74×** |
| 额外峰值字节 | 1,048,576（1 MiB，状态翻倍） | 2,097,152（16 B × 131,072 次写） | **0** |
| allocs/op | 1 | 1 | **0** |
| 吞吐（含回滚） | 1.12 GB/s | 0.30 GB/s | **1.52 GB/s** |
| 逆执行代价 | — | — | 293,714 ns ≈ forward 的 0.98× |
| 代码量 | 0 行生成 | 0 行生成（但每步一条 16 B 记录） | 逆函数 120 行 / 3,150 B |

### 第二批（同一台机器、系统负载更高时重跑）：绝对数变了 4–6 倍，排序与内存数没变

同一条命令、同一份代码再跑一遍（`run_all.ps1` 第 5 步），这组数字**比第一批慢 4–6 倍**
（本轮 `go test ./harness` 也从 3.0 s 涨到 20.6 s，说明是机器负载/降频，不是代码变了）：

| 方案 | run1 | run2 | run3 | 中位数 | B/op | allocs/op |
|---|---|---|---|---|---|---|
| `ForwardOnly` | 2,174,201 | 1,848,396 | 1,194,633 | **1,848,396** | 0 | 0 |
| `InverseOnly` | 1,119,157 | 914,004 | 722,903 | **914,004** | 0 | 0 |
| `SnapshotRollback` | 3,650,364 | 5,735,635 | 8,259,246 | **5,735,635** | 1,048,586 | 1 |
| `UndoLogRollback` | 15,365,992 | 17,958,774 | 18,749,827 | **17,958,774** | 2,097,155 | 1 |
| `ReversibleRollback` | 2,510,649 | 2,885,259 | 2,343,054 | **2,510,649** | **0** | **0** |

批 2 的比值：逆执行 **0.44×** snapshot，undo-log **3.13×**，逆/正 = 0.49×。

### 第三批（又在另一个负载状态重跑，`run_all.ps1` 第 5 步）

中位数：`ForwardOnly` 1,265,821 ns · `InverseOnly` 1,560,485 ns · `SnapshotRollback`
4,507,800 ns（1,048,586 B/op）· `UndoLogRollback` 7,196,062 ns（2,097,159 B/op）·
`ReversibleRollback` **1,461,889 ns、0 B、0 allocs**。逆执行 **0.32×** snapshot，
undo-log **1.60×**。

### 三批汇总（这就是我允许自己下的全部性能断言）

| 批次 | 逆执行 / snapshot | undo-log / snapshot | 逆 / 正 | 额外字节（snap / log / rev） |
|---|---|---|---|---|
| 1（空载） | **0.74×** | 3.74× | 0.98× | 1 MiB / 2 MiB / **0** |
| 2（高负载） | **0.44×** | 3.13× | 0.49× | 1 MiB / 2 MiB / **0** |
| 3（中负载） | **0.32×** | 1.60× | 1.23× | 1 MiB / 2 MiB / **0** |

**三批的排序完全一致**（`Reversible < Snapshot < UndoLog`），分配字节数逐次一致
（1 MiB / 2 MiB / 0），只有绝对延迟随机器负载漂移（同一函数跨批可差 4–6 倍）。
所以 H4 的判定只看比值区间与字节数，不看任何一次绝对采样：逆执行落在 snapshot 的
**0.32–0.74×**，undo-log 落在 **1.60–3.74×**；**3/3 批都是逆执行最快且零额外内存**。

**判定**：H4 的三条门槛全部满足——额外字节 0 ≤ 256 KiB；总延迟 0.32–0.74× ≤ 2.0×（三批）；
逆/正代码 1.01× ≤ 1.5×。在**这个**工作负载上，可逆执行是三项里唯一既省内存又不变慢的；
undo-log 最贵（每次写都要记 16 B，1 MiB 状态写完就是 2 MiB 日志，还多一轮随机写）。

**必须一起说的三条**（否则这张表会被过度解读）：

1. 计数器是 1.10 GHz 笔记本 CPU，**批与批之间绝对数差 4–6 倍**（热降频/系统负载），单批内三次也
   能差 2 倍。**只有排序和字节数稳定**：三批共 9 个 run，每一个都是
   `Reversible < Snapshot < UndoLog`，且 snapshot 恒分配 ≈1 MiB、undo-log ≈2 MiB、逆执行 0。
   比值本身三批同向（逆/snap：0.74 / 0.44 / 0.32），所以结论不依赖单次采样；但"1.5 GB/s"这类
   绝对吞吐不要当成该算法的性能，它只描述那批采样时的机器。
2. 这里的逆函数是 `roi` 里手写的（测的是**运行成本**，不是合成成本）；合成成本见 §4 的代码体积。
3. 前提是变换**真的可逆**。一旦区域里有日志写、channel send、外部调用，这条路直接不成立，
   就得回到 snapshot——这正是 §2 那些拒绝规则的用处。

---

## 6. 最有价值的部分：这个子集有多窄（H4b）

语料 24 条"真实形态"变换片段，**判定在跑之前就写在每个文件头**（`# expect: ...`），
`revsynth` 与 `TestCorpusPredictions` 双向核对：**24/24 预注册判定成立**。
结果：**7 条被接纳（29.2%）**，17 条被拒。

### 被拒的 17 条，按原因归类（这是"人话"版）

| 类别 | 条数 | 具体 | 为什么拒 |
|---|---|---|---|
| **数学上可逆、但不在逐语句代数里** | **3** | `x ^= x<<13` 的左/右半步（c03/c04）、`x *= 3`（c12） | 这是**代数太窄**，不是程序不可逆。xorshift 的每一步整体是双射，奇数乘法在 mod 2^64 上也有模逆元。要接纳它们，代数里得有"xor-shift 融合原语（含逆）"和 `mul` 原语（配奇数校验）。**这是我的选择，不是数学的限制。** |
| **真的丢信息** | 6 | `mask x,0xff`（c05）、`x=0`（c06）、非双射置换（c15）、偶数乘子仿射表（c11）、`x*=8`（c13）、**在标志寄存器上做条件交换（c07）** | 前 5 个没有争议。**c07 值得单说**：`if x0.bit0 { swap x0,x1 }` 是真实世界里最常见的可逆分支写法（CSWAP），但它**真的**不可逆——出口的 `x0` 可能是旧的 `x0` 也可能是旧的 `x1`，从结果无法区分"换过/没换"。这里的拒绝是**正确**的，不是保守误伤。正确写法是把标志与数据分开（c08，被接纳）。 |
| **退化恒等** | 2 | `rotl x,0`（c16）、`rotr x,64`（c17） | 恒等，逆不唯一（编译器常量折叠后常见），报 R010 提醒。 |
| **无逆向契约 / 效应 / 结构** | 6 | 任意查表（c09）、无界循环（c19）、写 `key`（c20）、切片别名（c21）、channel send（c22）、外部调用（c23） | 与 §2 的准入边界一致；`c10` 说明"有契约的查表"可以进。 |

### 一句话归因

**分支规则只拒了 1 条（c07），而那 1 条本来就不可逆；真正卡住可逆程序的是原语代数，
它拒了 3 条数学上可逆的常见写法（xorshift / 乘法混合）。**

所以，如果要把这个特性做下去，优先级应该是：**先扩代数**（融合的 xor-shift 原语、
奇数乘法、位级判别子证明），**再谈分支规则**。我最初担心"判别子保留"会把子集掐死，
数据显示这个担心是错的，代价只有 1/24。

另有一条**我预注册时没想到、实测出来的**保守性：分支规则是"寄存器粒度 + 语法写没写"，
所以像 `if bit x0,3 { x0 ^= 0x... }` 这种"只动别的位"的写法也会被拒（`R004`）。
语料里没有这样的样本，**因此我没有量化它多拒了多少本来可逆的程序——这一项未验证。**

---

## 7. 结论

1. **精确撤销是真的、且便宜**：1,011,680 条记录、64 字节逐字节 0 失败；三路实现
   （发射代码 / 解释器 / 手写参考）互相独立且一致；分支判别子保留规则在语义上成立
   （出口状态能重建分支选择，因为判别子寄存器根本没被写）。
2. **ROI 方向是正的，但只在正确的场合**：1 MiB 上 0 额外字节、0 alloc、总延迟 0.32–0.74× snapshot
   （三批）；undo-log 反而最贵（2 MiB + 1.60–3.74×）。前提是"状态真的可逆 + 无外部副作用"。
3. **负面结论（最有价值的一条）**：在 24 条真实形态语料里只接纳 7 条（29.2%），
   而**卡点是原语代数不是分支规则**——3 条被拒的片段数学上本来可逆。这个子集目前
   **对真实密码/变换代码偏窄**；要变有用，得先把代数扩到 xorshift / 乘法 / 位级判别子。
4. **诚实的定位**：可逆区域适合"纯算术、owned、单 task、无副作用"的投机计算，
   不适合替代通用回滚。issue 自己也把"有些场景 snapshot 更便宜更安全"列为可接受的证伪——
   在非可逆或含副作用的场景，数据（undo-log 3.74×、snapshot 1 MiB）支持回到 snapshot。

---

## 8. 没做什么 / 未验证

- **不是 Hybrid 前端**：`.rev` 是我自定的最小 IR 语法，不是 `.hy`；没有改 `Go-Hybrid` 仓库，
  没有任何上游集成声明。
- **不撤销外部副作用**：网络、文件、channel、goroutine、panic、崩溃都不在模型内；遇到即拒。
- **无并发**：区域假定单线程；没有测多 task 同时进入、没有测"另一个 task 观察到中途状态"。
- **无循环**：`loop` 直接拒（R011）；所以没有"状态相关的循环次数"这一整类程序。
- **未验证编译器优化的影响**：没有测内联/向量化/常量折叠是否会破坏逆契约（issue 明确列为风险）。
- **未量化位级判别子规则**：`R004` 是寄存器粒度的保守规则；位级精化能多接纳多少程序，未验证。
- **ROI 只测了一个工作负载**：一次 ARX 扫描、1 MiB、顺序访问。随机访问、多轮密码、
  或远小于 cache 的状态，排序可能变；也未测多 goroutine 竞争。
- **单机三批采样**：1.10 GHz 笔记本 CPU，各 `-count=3`；批间绝对数差 4–6 倍。没有跨机器、
  没有 pin CPU、没有关掉其它负载，所以只能断言**排序与比值区间**，不能断言绝对吞吐。
- **没有实现 S-box 的通用表声明**（只支持 `affine A B` 形式），也没有测 256 项字面量表。
- **没有做泛化**：只测 8 个 64 位寄存器 + 1 个 key；没测变长状态、结构体字段、字节切片。

---

## 9. 复现

```powershell
cd C:\Users\v\surfcore\state\agents\bounty\work\w-reversible-regions-4596
C:\Users\v\golang\go\bin\go.exe run ./cmd/revsynth          # 44 条预注册判定 + 发射 gen/
C:\Users\v\golang\go\bin\go.exe vet ./...                   # 干净
C:\Users\v\golang\go\bin\go.exe test ./... -count=1         # 1,011,680 条记录
C:\Users\v\golang\go\bin\go.exe test ./roi -run TestSchemesRestoreExactly -bench . -benchmem -count=3
# 或：powershell -ExecutionPolicy Bypass -File .\run_all.ps1
```

关键输出（原文见 `out/revsynth.txt`、`out/summary.json`）：

```
负例诊断码互不相同：10/10 unique
语料接纳率：7/24 = 29.2%
合计 forward=3127 B  inverse=3150 B  ratio=1.01
预测审计：44/44 条预注册判定成立；写出 gen/gen_cases.go 与 out/summary.json

TOTAL: 1011680 records, 1011680 inverse executions, 3-way agreement (emitted / interpreter / hand-written)
ok  revreg/harness
diagnostics exercised: 14/14 catalogue entries

BenchmarkSnapshotRollback-8     1048584 B/op   1 allocs/op   ~936,970 ns/op
BenchmarkUndoLogRollback-8      2097158 B/op   1 allocs/op  ~3,504,574 ns/op
BenchmarkReversibleRollback-8         0 B/op   0 allocs/op    ~689,871 ns/op
```
