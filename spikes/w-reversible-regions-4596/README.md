# w-reversible-regions-4596

issue [#4596](https://github.com/limitlessprojects/Go-Hybrid/issues/4596) 的 wildcard spike：
**把一小块区域做成"由构造保证可逆"，编译器同时发射正向与机械推导的逆函数，用逆执行代替快照/日志。**

这里**不是** Hybrid 前端，也不声称上游实现。它是一个独立的最小实验：自定一个可逆 IR、
一个准入检查器、一个前向解释器、一个逆合成器、一个 Go 发射器，然后按 issue 的验收方式
把它测穿——包括预先声明失败条件，以及诚实地报告这个子集有多窄。

**结论（详见 [RESULTS.md](RESULTS.md)）：** 精确撤销这条路在 10 个程序上 101 万条记录
逐字节无差，1 MiB 上回滚比 snapshot 少 1 MiB 且总延迟更低（0.32–0.74×，三批），但**真正的
瓶颈是原语代数，不是分支规则**：24 条常见变换里只有 7 条（29.2%）能被接纳，而被拒的 17 条
里有 3 条其实在数学上可逆（xorshift 两半、奇数乘法），只是不在逐语句代数里。

## 复现

```powershell
cd C:\Users\v\surfcore\state\agents\bounty\work\w-reversible-regions-4596

# 1) 检查 44 条预注册判定，然后把 10 个正例发射成 gen/gen_cases.go
C:\Users\v\golang\go\bin\go.exe run ./cmd/revsynth

# 2) 编译 + 全部测试（round-trip 1,011,680 条记录、负例、语料）
C:\Users\v\golang\go\bin\go.exe vet ./...
C:\Users\v\golang\go\bin\go.exe test ./... -count=1

# 3) ROI：1 MiB 上 snapshot / undo-log / 逆执行
C:\Users\v\golang\go\bin\go.exe test ./roi -run TestSchemesRestoreExactly -bench . -benchmem -count=3
```

或者一次跑完：`powershell -ExecutionPolicy Bypass -File .\run_all.ps1`
（顺序同上，任一非零退出码即中止）。

`go build ./...` 通过即证明：发射出来的正向与逆向函数本身能作为 Go 编译。
`gen/` 是**生成目录**，`cmd/revsynth` 会整文件覆盖，不要手改。

## 目录

| 路径 | 作用 |
|---|---|
| `PREREG.md` | 实现前写死的 H1–H4、证伪条件、诊断码目录（未回头修改） |
| `RESULTS.md` | 结果：实测数字、20 例表、语料窄度、ROI、结论、没做什么 |
| `ir/ir.go` | IR 类型、S-box 代数（仿射表 + 双射校验 + 表求逆）、案例语法解析 |
| `ir/check.go` | 准入检查器：白名单 + 14 个具名诊断码，fail closed |
| `ir/interp.go` | 前向解释器（带分支轨迹，用于覆盖率断言） |
| `ir/invert.go` | 逆合成器：整段反序 + 逐原语取逆 + 分支保持判别子 |
| `ir/emit.go` | Go 发射器：正向/逆向函数、置换按轮换发射、S-box 表与辅助函数 |
| `refimpl/ref.go` | **独立**参考实现：手写正/逆变换，S-box 逆用模逆元代数推导 |
| `cmd/revsynth/` | 驱动：判定、发射、代码体积表、`out/summary.json` |
| `cases/pos/*.rev` | 10 个正例（可读源） |
| `cases/neg/*.rev` | 10 个负例，每个一个诊断码 |
| `corpus/*.rev` | 24 条"常见变换"语料，含预注册判定，用于量化子集窄度 |
| `gen/gen_cases.go` | 生成物：`Forward<Case>` / `Inverse<Case>` + 注册表 |
| `harness/` | 验收测试：三路对账、边界语料、分支覆盖率、负例区分性 |
| `roi/` | 1 MiB 三种回滚方案的 benchmark 与语义测试 |
| `out/` | `revsynth.txt`（原始输出）、`summary.json`（机器可读） |

## 案例语法（我的 IR，不是 `.hy`）

```
program name
sbox S1 affine 0x1d 0x63          # 声明表 y = (x*A mod 256) ^ B，检查器独立验证双射
add  x0, 0x9e3779b97f4a7c15       # x0 += k        (mod 2^64)
sub  x1, 3                        # x1 -= k
addv x0, x1                       # x0 += x1       (要求 dst != src)
subv x0, x2                       # x0 -= x2
xor  x0, x3                       # x0 ^= x3
xori x0, key                      # x0 ^= key      (key 是只读保留字)
rotl x0, 7      / rotr x1, 13     # 循环移位
swap x0, x1                       # 自逆
perm 2 0 3 1 5 4 7 6              # new[i] = old[p[i]]，必须双射
sboxb x0, S1                      # 逐字节代换（逆表由表求逆得到）
if bit x7, 0 == 0   ... else ... end
# 会被拒绝的语句（为了让诊断可命名而可解析）：
set mask shl shr mul lookup loop call chansend alias capture
```

`# expect: ADMIT` / `# expect: R0xx_CODE` 是预注册判定，`revsynth` 与测试都会核对。

## 三条最容易误读的地方

1. **分支规则是语法级、寄存器粒度**：分支臂内只要写了判别子寄存器就拒（R004），
   不做"位被保留"的语义证明。比必要更保守；代价见 RESULTS 的语料量化。
2. **逆函数不是"再来一次正向"**：是逐原语代数取逆 + 整段反序；`addv/subv` 的正确性
   依赖逆序执行（撤销第 i 步时状态正好等于第 i 步之后）。
3. **只撤销值状态**：网络、文件、channel、goroutine、panic 一律不在模型内，遇到即拒。
