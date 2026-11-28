# One command: regenerate + verify the whole spike.
#   powershell -ExecutionPolicy Bypass -File .\run_all.ps1
# Requires: Go 1.27.4 at the path below (standard library only, no network).
$ErrorActionPreference = 'Continue'
$go = 'C:\Users\v\golang\go\bin\go.exe'
Push-Location $PSScriptRoot

Write-Host '=== 1/5 go run ./cmd/revsynth (admission + emit gen/gen_cases.go) ==='
& $go run ./cmd/revsynth
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

Write-Host "`n=== 2/5 go vet ./... ==="
& $go vet ./...
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

Write-Host "`n=== 3/5 go build ./... ==="
& $go build ./...
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

Write-Host "`n=== 4/5 go test ./... -count=1 (round-trip + negatives + corpus) ==="
& $go test ./... -count=1
if ($LASTEXITCODE -ne 0) { Pop-Location; exit $LASTEXITCODE }

Write-Host "`n=== 5/5 go test ./roi -bench . -benchmem (1 MiB rollback ROI) ==="
& $go test ./roi -run 'TestSchemesRestoreExactly|TestUndoRecordSize' -bench . -benchmem -count=3
$code = $LASTEXITCODE
Pop-Location
exit $code
