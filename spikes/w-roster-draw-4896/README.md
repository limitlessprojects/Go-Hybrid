# w-roster-draw-4896 — 把「带私有选择的策略」编译成公平抽取器

wildcard track spike，对应 `limitlessprojects/Go-Hybrid` issue **#4896**
（*Compile a fair roster draw from a validator with hidden choices*）。

一句话：**issue 的 151 / 284 / 43-84-23-1 全部独立复现；「对 (roster,handoff) 对均匀抽」把
ABABABAB 抬到 ABACACBA 的 4.57 倍；「先均匀抽 roster 再随便挑 handoff」的 roster 边际是对的，
但 reveal 的条件熵 0.811 bit、重复抽取一致率只有 0.615，私有选择会漏到公开输出；确定化 +
大整数计数 + rank 分块给出的 151 个 roster 上 χ²=120.0（df=150, p=0.966），且对「把私有分支写两遍」
这种等价重构逐 rank 完全不变。**

## 目录

```
PREREG.md                     预注册：H1–H4 与证伪条件（写于跑数之前）
README.md                     本文件
RESULTS.md                    结果、对照表、结论、没做什么
src/hybrid_roster/
    policy.py                 策略：私有状态 (上一位, 各人次数, 交接状态) + step/accepting
    determinize.py            子集构造、任意精度后缀计数、rank 编解码、canonical witness、PathCounter 对照
    oracle.py                 独立穷举器（DFS 具体词 + 词面判据；另有一个 product 版互证）
    samplers.py               四个抽取器：pairs-uniform / path-uniform / roster-then-handoff / determinized
    stats.py                  χ² 生存函数、Wilson 区间、风险比 CI、熵
src/experiments/
    run_experiments.py        跑完 H1–H4，写 out/results.json 与 out/tables.md
tests/test_all.py             20 个测试，一条命令跑通（含 1/151 的 χ² 检验，固定种子）
out/                          results.json / tables.md / run.log / tests.log
```

## 复现

无第三方依赖，Python 3.12（3.10+ 应该都行），不联网。

```powershell
cd C:\Users\v\surfcore\state\agents\bounty\work\w-roster-draw-4896

python src/experiments/run_experiments.py   # 约 1–2 分钟（n=32 的完整状态计数较慢）
python tests/test_all.py                    # 约 3 秒，20 个测试
```

`run_experiments.py` 会重新生成 `out/results.json` 和 `out/tables.md`（RESULTS.md 里的表就是它的输出）。
种子策略：`MASTER = 20261128`，每个配置的种子 = `int(sha256(f"{MASTER}|{config_id}").hexdigest()[:16], 16)`，
所以同一条命令在任何机器上给出同样的抽样。

## 模型（和 issue 的对应关系）

- **可见字母表** = 志愿者名字；**可见词** = 公开的 roster。
- **私有状态** = `(上一位, 各人已用次数, 交接状态)`；交接状态是私有选择，不进公开输出。
- **约束**：相邻不同人；`C ≤ 2`；至少一处 `A→B` 交接可被指定（策略 A，8 班）。
- **合法 (roster,handoff) 对** = roster 合法且从它的某个 `A→B` 边界里挑一个作为培训位。

`SetAutomaton` 对可见符号做子集构造：`delta(S, a) = ∪ step(s, a)`（集合，重复自动去重），
于是每个可见词在确定化自动机上只有一条路径，`count(S, r)` 数的是**不同的可见后缀**而不是路径。
rank 0..N-1 按 `delta` 的分块顺序解码，每个可见词恰好占一个 rank。

**canonical witness**：给定解码出的 roster，从左到右找第一个「强制指定该边界后仍然接受」的边界。
它是可见词的函数，所以 reveal 不含任何私有随机性（这是 H3 的关键）。

**一个容易踩的坑（本 spike 的实际发现之一）**：计数的私有状态不能带「交接是几步之前选的」这种年龄信息。
带上它，residual set 几乎每个前缀都不同，计数表退化成可见词集合的 trie，n=32 时要 40 万条表项、20–47 秒
（issue 自己设的 4096 子集上限在 n≈10 就爆）。只保留「是否已选」的布尔量（issue 原文用的正是布尔量），
集合峰值恒为 2，n=64 只要 3007 条表项、0.05 秒；witness 用单词回放单独恢复即可。
见 `RESULTS.md` 的 H4 与 `out/tables.md`。

## 三种（实际是四种）抽取器

| 类 | 抽的是什么 | 用于 |
|---|---|---|
| `PairsUniform` | 显式列出所有 (roster, handoff) 对，均匀抽 | 错误实现 1 |
| `PathUniform` | 按接受**路径**数排名解码 | 错误实现 1b（编译器最可能写出的版本） |
| `RosterThenHandoff` | 先均匀抽 roster，再在合法 handoff 里均匀挑（`reveal="boundary"` 按位置、`"run"` 按路径） | 错误实现 2 |
| `Determinized` | 均匀 rank → 解码 roster → canonical witness | 提案 |

`PairsUniform` 和 `PathUniform` 是两套完全不同的代码，但应当同分布——实测一致，这本身是一条交叉证据。

## 说明

- 不碰 `Go-Hybrid` 仓库，不跑 git，不联网，只写本目录。
- 计时是本机单次测量的结果（Windows / CPython 3.12），有 ±2 倍噪声，量级可信、小数位不可信。
