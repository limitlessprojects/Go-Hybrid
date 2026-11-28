# w-transfer-plan-4634

`ISSUE #4634` wildcard spike：为「一维有序网格之间搬运分段常数场」实现一个普通 Go 库，让 plan 同时携带 write partition 与守恒律，然后回答 issue 里那句判定条件——**普通 Go 库能不能绑定网格身份、在同等调用方成本下强制同样约束**。

交付内容：`python/model.py`（独立重跑作者模型，1536 / 4608 与全部负数控制自检）、`go-control/`（标准库 Go 模块 `wtransfer`，两种调用形状 + 独立 oracle + 全组合穷举测试 + 基准）、`RESULTS.md`（预先声明的 H1–H3、实测表格、结论、成本、没做什么）、`logs/`（本文所有数字的原始输出，未编辑）。

## 复现命令

```powershell
# 1) Python 参考模型（独立重写，含 1536/4608 自检与负数控制）
cd C:\Users\v\surfcore\state\agents\bounty\work\w-transfer-plan-4634
python python\model.py
# 期望最后一行：SELF-CHECK PASSED

# 2) Go：全部测试（含两个可运行 example）
cd go-control
go test ./... -count=1 -v

# 3) Go：基准（plan 校验 vs 重复 apply）
go test ./xfer -run XXX -bench . -benchmem -count=1
```

Go 模块只用标准库（`math/big`、`hash/fnv`），`go.mod` 内 `go 1.22`，无外部依赖，不联网。
本机实测环境：`go version go1.27.4 windows/amd64`，Python 3.12.10。

## 目录

| 路径 | 内容 |
| --- | --- |
| `python/model.py` | 独立重写的参考模型：sparse plan、两个互不相同的 oracle、N1–N5 负数控制、1536/4608 计数断言 |
| `go-control/xfer/mesh.go` | `Mesh`：边序列 + 指纹身份（FNV-1a over edges），身份是几何而不是指针 |
| `go-control/xfer/plan.go` | `Plan`：绑定两侧网格、每目标单元一行、精确有理权重；`BuildPlan` / `AdmitPlan` / `Validate` / `CheckIntegral` |
| `go-control/xfer/apply.go` | `Run`（写分区记账）、回调式 `ApplyRows`、生成循环式 `GatherRows`、`RowView`、未绑定控制 `ApplyUnbound` |
| `go-control/xfer/field.go` | `Field`：把网格身份和数值绑在一起的场类型（`ApplyFields` / `GatherFields`） |
| `go-control/xfer/oracle.go` | 两个独立 oracle：解析积分、单位栅格；以及场的精确积分/比较工具 |
| `go-control/xfer/*_test.go` | 穷举、组合陷阱、写分区守卫、负数控制、基准、可运行 example |
| `logs/` | `python-model.txt`、`go-test.txt`、`bench-run1..3.txt` 原始输出 |
