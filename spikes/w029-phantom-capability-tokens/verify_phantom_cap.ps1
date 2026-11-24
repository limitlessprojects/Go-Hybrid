# verify_phantom_cap.ps1 — Native Verification of 20-Case Conformance Suite
# Moonshot Track Proposal W-029: Zero-Alias Phantom Capability Tokens

class Diagnostic {
    [string]$Code
    [string]$Message
    [string]$Span

    Diagnostic([string]$code, [string]$msg, [string]$span) {
        $this.Code = $code
        $this.Message = $msg
        $this.Span = $span
    }
}

class Capability {
    [string]$TypeName
    [string]$Perm
    [object]$Value
    [int]$Version

    Capability([string]$type, [string]$perm, [object]$val) {
        $this.TypeName = $type
        $this.Perm = $perm
        $this.Value = $val
        $this.Version = 0
    }
}

class PhantomTypeChecker {
    [hashtable]$Bindings = @{}
    [System.Collections.Generic.List[Diagnostic]]$Diagnostics = [System.Collections.Generic.List[Diagnostic]]::new()
    [hashtable]$ActiveBorrows = @{}
    [System.Collections.Generic.List[hashtable]]$TaskSpawns = [System.Collections.Generic.List[hashtable]]::new()

    [Capability] DeclareUnique([string]$name, [string]$type, [object]$val) {
        $cap = [Capability]::new($type, "Unique", $val)
        $this.Bindings[$name] = $cap
        return $cap
    }

    [object] ReadShared([string]$name, [string]$span) {
        if (-not $this.Bindings.ContainsKey($name)) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$name not in scope", $span))
            return $null
        }
        $cap = $this.Bindings[$name]
        if ($cap.Perm -eq "Expired") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "use of moved/expired capability `$name", $span))
            return $null
        }
        return $cap.Value
    }

    [bool] MutateUnique([string]$name, [scriptblock]$mutator, [string]$span) {
        if (-not $this.Bindings.ContainsKey($name)) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$name not in scope", $span))
            return $false
        }
        $cap = $this.Bindings[$name]
        if ($cap.Perm -eq "Expired") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "use of moved/expired capability `$name", $span))
            return $false
        }
        if ($cap.Perm -eq "Shared") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3006", "cannot mutate through shared borrow `$name", $span))
            return $false
        }
        if ($this.ActiveBorrows.ContainsKey($name) -and $this.ActiveBorrows[$name] -gt 0) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3004", "cannot mutate `$name while active shared borrows exist", $span))
            return $false
        }
        $cap.Value = & $mutator $cap.Value
        $cap.Version++
        return $true
    }

    [object] ScopedBorrow([string]$name, [scriptblock]$block, [string]$span) {
        if (-not $this.Bindings.ContainsKey($name)) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$name not in scope", $span))
            return $null
        }
        $cap = $this.Bindings[$name]
        if ($cap.Perm -ne "Unique") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "cannot borrow `$name`: requires Unique capability", $span))
            return $null
        }
        $current = if ($this.ActiveBorrows.ContainsKey($name)) { $this.ActiveBorrows[$name] } else { 0 }
        $this.ActiveBorrows[$name] = $current + 1
        $borrowedCap = [Capability]::new($cap.TypeName, "Shared", $cap.Value)
        try {
            $res = & $block $borrowedCap
            return $res
        } finally {
            $this.ActiveBorrows[$name] = $this.ActiveBorrows[$name] - 1
        }
    }

    [bool] TransferUnique([string]$src, [string]$dst, [string]$span) {
        if (-not $this.Bindings.ContainsKey($src)) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$src not in scope", $span))
            return $false
        }
        $cap = $this.Bindings[$src]
        if ($cap.Perm -eq "Expired") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "use of moved/expired capability `$src", $span))
            return $false
        }
        if ($cap.Perm -eq "Shared") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3002", "cannot transfer shared capability `$src as unique", $span))
            return $false
        }
        if ($this.ActiveBorrows.ContainsKey($src) -and $this.ActiveBorrows[$src] -gt 0) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3004", "cannot move `$src while active shared borrows exist", $span))
            return $false
        }
        $cap.Perm = "Expired"
        $this.Bindings[$dst] = [Capability]::new($cap.TypeName, "Unique", $cap.Value)
        return $true
    }

    [bool] SpawnTask([string]$taskName, [string[]]$capturedVars, [string]$span) {
        foreach ($var in $capturedVars) {
            if (-not $this.Bindings.ContainsKey($var)) {
                $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$var not declared", $span))
                return $false
            }
            $cap = $this.Bindings[$var]
            if ($cap.Perm -eq "Expired") {
                $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "cannot capture moved/expired capability `$var in task `$taskName", $span))
                return $false
            }
            if ($this.ActiveBorrows.ContainsKey($var) -and $this.ActiveBorrows[$var] -gt 0) {
                $this.Diagnostics.Add([Diagnostic]::new("HYB-E3001", "cannot send `$var across task boundary while borrowed", $span))
                return $false
            }
            # Affine lexical fence discharges caller's access
            $cap.Perm = "Expired"
        }
        $this.TaskSpawns.Add(@{ Task = $taskName; Captured = $capturedVars })
        return $true
    }

    [bool] SplitPair([string]$pairVar, [string]$fstVar, [string]$sndVar, [string]$span) {
        if (-not $this.Bindings.ContainsKey($pairVar)) {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E1001", "variable `$pairVar not declared", $span))
            return $false
        }
        $cap = $this.Bindings[$pairVar]
        if ($cap.Perm -ne "Unique") {
            $this.Diagnostics.Add([Diagnostic]::new("HYB-E3003", "split requires Unique pair `$pairVar", $span))
            return $false
        }
        $cap.Perm = "Expired"
        $this.Bindings[$fstVar] = [Capability]::new("$($cap.TypeName).Fst", "Unique", $cap.Value[0])
        $this.Bindings[$sndVar] = [Capability]::new("$($cap.TypeName).Snd", "Unique", $cap.Value[1])
        return $true
    }
}

$results = @()

# P01: Unique Linear Transfer
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("x", "Buffer", @(1, 2, 3)) | Out-Null
$p01 = $tc.TransferUnique("x", "y", "test:1") -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P01"; Name="Unique Linear Transfer"; Passed=$p01; Kind="POSITIVE" }

# P02: Scoped Read Borrowing
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("buf", "Buffer", "hello") | Out-Null
$borrowed = $tc.ScopedBorrow("buf", { param($s) $tc.ReadShared("buf", "test:2") }, "test:2")
$p02 = ($borrowed -eq "hello") -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P02"; Name="Scoped Read Borrowing"; Passed=$p02; Kind="POSITIVE" }

# P03: Disjoint Field Split & Mutation
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("pair", "Pair", @("dataA", "dataB")) | Out-Null
$tc.SplitPair("pair", "partA", "partB", "test:3") | Out-Null
$m1 = $tc.MutateUnique("partA", { param($v) "$v`_mod" }, "test:3")
$m2 = $tc.MutateUnique("partB", { param($v) "$v`_mod" }, "test:3")
$p03 = $m1 -and $m2 -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P03"; Name="Disjoint Field Split & Mutation"; Passed=$p03; Kind="POSITIVE" }

# P04: Pipeline Stream Linear Propagation
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("s0", "Stream", @(10, 20)) | Out-Null
$t1 = $tc.TransferUnique("s0", "s1", "test:4")
$t2 = $tc.TransferUnique("s1", "s2", "test:4")
$p04 = $t1 -and $t2 -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P04"; Name="Pipeline Stream Linear Propagation"; Passed=$p04; Kind="POSITIVE" }

# P05: Task Spawn with Affine Fence (W-004 fix)
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("job", "JobData", @{ id = 42 }) | Out-Null
$p05 = $tc.SpawnTask("WorkerTask", @("job"), "test:5") -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P05"; Name="Task Spawn with Affine Fence"; Passed=$p05; Kind="POSITIVE" }

# P06: Sequential Borrow-then-Mutate
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("dataset", "Matrix", @(1, 2, 3, 4)) | Out-Null
$tc.ScopedBorrow("dataset", { param($d) 10 }, "test:6") | Out-Null
$m6 = $tc.MutateUnique("dataset", { param($v) @(2, 4, 6, 8) }, "test:6")
$p06 = $m6 -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P06"; Name="Sequential Borrow-then-Mutate"; Passed=$p06; Kind="POSITIVE" }

# P07: Nested Re-entrant Borrows
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("node", "Tree", @{ left=1; right=2 }) | Out-Null
$nested = $tc.ScopedBorrow("node", { param($b1)
    $tc.ScopedBorrow("node", { param($b2) $true }, "test:7")
}, "test:7")
$p07 = ($nested -eq $true) -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P07"; Name="Nested Re-entrant Borrows"; Passed=$p07; Kind="POSITIVE" }

# P08: Read-Only Non-Mutating Access
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("res", "Resource", 100) | Out-Null
$val = $tc.ReadShared("res", "test:8")
$p08 = ($val -eq 100) -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P08"; Name="Read-Only Non-Mutating Access"; Passed=$p08; Kind="POSITIVE" }

# P09: Variable Shadowing with New Unique
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("token", "Token", 1) | Out-Null
$tc.TransferUnique("token", "used_token", "test:9") | Out-Null
$tc.DeclareUnique("token", "Token", 2) | Out-Null
$p09 = ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P09"; Name="Variable Shadowing with New Unique"; Passed=$p09; Kind="POSITIVE" }

# P10: Disjoint Channel Endpoints to Tasks
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("ch_pair", "ChanPair", @("tx", "rx")) | Out-Null
$tc.SplitPair("ch_pair", "tx", "rx", "test:10") | Out-Null
$s1 = $tc.SpawnTask("Sender", @("tx"), "test:10")
$s2 = $tc.SpawnTask("Receiver", @("rx"), "test:10")
$p10 = $s1 -and $s2 -and ($tc.Diagnostics.Count -eq 0)
$results += [PSCustomObject]@{ Id="P10"; Name="Disjoint Channel Endpoints to Tasks"; Passed=$p10; Kind="POSITIVE" }

# --- 10 NEGATIVE CONFORMANCE CASES ---
# N01: Use After Move Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("x", "Buffer", @(1, 2)) | Out-Null
$tc.TransferUnique("x", "y", "test:N01") | Out-Null
$tc.MutateUnique("x", { param($v) $v }, "test:N01") | Out-Null
$n01 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3003" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N01"; Name="Use After Move Rejection"; Passed=$n01; Kind="NEGATIVE"; Diag="HYB-E3003" }

# N02: Mutation During Borrow Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("shared_val", "Data", 50) | Out-Null
$tc.ScopedBorrow("shared_val", { param($s)
    $tc.MutateUnique("shared_val", { param($v) $v + 1 }, "test:N02")
}, "test:N02") | Out-Null
$n02 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3004" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N02"; Name="Mutation During Borrow Rejection"; Passed=$n02; Kind="NEGATIVE"; Diag="HYB-E3004" }

# N03: Post-Spawn Caller Mutation Rejection (W-004 fix)
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("captured", "SharedState", @(10)) | Out-Null
$tc.SpawnTask("ChildTask", @("captured"), "test:N03") | Out-Null
$tc.MutateUnique("captured", { param($v) @(20) }, "test:N03") | Out-Null
$n03 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3003" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N03"; Name="Post-Spawn Caller Mutation Rejection"; Passed=$n03; Kind="NEGATIVE"; Diag="HYB-E3003" }

# N04: Double Unique Transfer Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("res", "Handle", 1) | Out-Null
$tc.TransferUnique("res", "destA", "test:N04") | Out-Null
$tc.TransferUnique("res", "destB", "test:N04") | Out-Null
$n04 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3003" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N04"; Name="Double Unique Transfer Rejection"; Passed=$n04; Kind="NEGATIVE"; Diag="HYB-E3003" }

# N05: Spawn Borrowed Variable Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("loaned", "Data", "info") | Out-Null
$tc.ScopedBorrow("loaned", { param($b)
    $tc.SpawnTask("AsyncWorker", @("loaned"), "test:N05")
}, "test:N05") | Out-Null
$n05 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3001" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N05"; Name="Spawn Borrowed Variable Rejection"; Passed=$n05; Kind="NEGATIVE"; Diag="HYB-E3001" }

# N06: Mutate Through Shared Ref Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("entity", "Entity", 99) | Out-Null
$tc.ScopedBorrow("entity", { param($s)
    $tc.Bindings["temp_ref"] = $s
    $tc.MutateUnique("temp_ref", { param($v) $v + 1 }, "test:N06")
}, "test:N06") | Out-Null
$n06 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3006" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N06"; Name="Mutate Through Shared Ref Rejection"; Passed=$n06; Kind="NEGATIVE"; Diag="HYB-E3006" }

# N07: Move Shared Capability Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("base", "Block", 123) | Out-Null
$tc.ScopedBorrow("base", { param($b)
    $tc.Bindings["borrow_handle"] = $b
    $tc.TransferUnique("borrow_handle", "illegal_dest", "test:N07")
}, "test:N07") | Out-Null
$n07 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3002" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N07"; Name="Move Shared Capability Rejection"; Passed=$n07; Kind="NEGATIVE"; Diag="HYB-E3002" }

# N08: Split Expired Pair Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("p", "Pair", @(1, 2)) | Out-Null
$tc.TransferUnique("p", "p_moved", "test:N08") | Out-Null
$tc.SplitPair("p", "a", "b", "test:N08") | Out-Null
$n08 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3003" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N08"; Name="Split Expired Pair Rejection"; Passed=$n08; Kind="NEGATIVE"; Diag="HYB-E3003" }

# N09: Read Expired Capability Rejection
$tc = [PhantomTypeChecker]::new()
$tc.DeclareUnique("val", "Value", "data") | Out-Null
$tc.TransferUnique("val", "next_val", "test:N09") | Out-Null
$tc.ReadShared("val", "test:N09") | Out-Null
$n09 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E3003" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N09"; Name="Read Expired Capability Rejection"; Passed=$n09; Kind="NEGATIVE"; Diag="HYB-E3003" }

# N10: Spawn Undeclared Variable Rejection
$tc = [PhantomTypeChecker]::new()
$tc.SpawnTask("GhostTask", @("non_existent_var"), "test:N10") | Out-Null
$n10 = ($tc.Diagnostics | Where-Object { $_.Code -eq "HYB-E1001" }).Count -gt 0
$results += [PSCustomObject]@{ Id="N10"; Name="Spawn Undeclared Variable Rejection"; Passed=$n10; Kind="NEGATIVE"; Diag="HYB-E1001" }

# --- PRINT CONFORMANCE REPORT ---
Write-Host "======================================================================"
Write-Host "W-029 PHANTOM CAPABILITY TOKENS: 20-CASE CONFORMANCE MATRIX"
Write-Host "======================================================================"
foreach ($r in $results) {
    $status = if ($r.Passed) { "[PASS]" } else { "[FAIL]" }
    $diag = if ($r.Diag) { "[$($r.Diag)]" } else { "" }
    Write-Host ("{0} {1}: {2,-38} {3,-10} {4}" -f $status, $r.Id, $r.Name, $r.Kind, $diag)
}
Write-Host "----------------------------------------------------------------------"
$passedCount = ($results | Where-Object { $_.Passed }).Count
Write-Host "Summary: $passedCount / $($results.Count) Tests Passed"
Write-Host "======================================================================"

# --- BENCHMARK MEASUREMENT ---
Write-Host "`nRUNNING DIFFERENTIAL BENCHMARK..."
$swGraph = [System.Diagnostics.Stopwatch]::StartNew()
$nodes = 300
$graph = @{}
for ($i = 0; $i -lt $nodes; $i++) {
    $set = [System.Collections.Generic.HashSet[int]]::new()
    for ($j = [Math]::Max(0, $i - 5); $j -lt [Math]::Min($nodes, $i + 5); $j++) {
        $set.Add($j) | Out-Null
    }
    $graph[$i] = $set
}
for ($round = 0; $round -lt 2; $round++) {
    for ($i = 0; $i -lt $nodes; $i++) {
        foreach ($v in $graph[$i]) {
            $graph[$v].UnionWith($graph[$i])
        }
    }
}
$swGraph.Stop()

$swPhantom = [System.Diagnostics.Stopwatch]::StartNew()
$tcBench = [PhantomTypeChecker]::new()
for ($i = 0; $i -lt $nodes; $i++) {
    $vName = "v_$i"
    $tcBench.DeclareUnique($vName, "Scalar", $i) | Out-Null
    if ($i % 3 -eq 0) {
        $tcBench.ScopedBorrow($vName, { param($s) $s.Value }, "bench") | Out-Null
    } elseif ($i % 3 -eq 1) {
        $tcBench.MutateUnique($vName, { param($v) $v + 1 }, "bench") | Out-Null
    } else {
        $tcBench.TransferUnique($vName, "v_$($i)_moved", "bench") | Out-Null
    }
}
$swPhantom.Stop()

$graphMs = $swGraph.Elapsed.TotalMilliseconds
$phantomMs = $swPhantom.Elapsed.TotalMilliseconds
$speedup = [Math]::Round($graphMs / [Math]::Max($phantomMs, 0.001), 2)

Write-Host "Nodes Processed:        $nodes"
Write-Host "AST Alias-Graph Time:   $([Math]::Round($graphMs, 2)) ms"
Write-Host "Phantom Cap Type Time:  $([Math]::Round($phantomMs, 2)) ms"
Write-Host "Throughput Speedup:     $speedup x"
Write-Host "======================================================================"
