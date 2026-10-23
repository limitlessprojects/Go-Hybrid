package main

import (
	"bytes"
	"fmt"
	"os"
)

type WasmType struct {
	Params  []byte
	Results []byte
}

type WasmExport struct {
	Name  string
	Kind  byte
	Index uint32
}

type WasmGlobal struct {
	Type    byte
	Mutable bool
	InitVal int64
}

type WasmDataSegment struct {
	Offset uint32
	Data   []byte
}

func readLEB128u(data []byte, pos *int) uint32 {
	var res uint32
	var shift uint
	for {
		b := data[*pos]
		*pos++
		res |= uint32(b&0x7f) << shift
		if b&0x80 == 0 {
			break
		}
		shift += 7
	}
	return res
}

func readLEB128s(data []byte, pos *int) int64 {
	var res int64
	var shift uint
	var b byte
	for {
		b = data[*pos]
		*pos++
		res |= int64(b&0x7f) << shift
		shift += 7
		if b&0x80 == 0 {
			break
		}
	}
	if shift < 64 && (b&0x40) != 0 {
		res |= -(1 << shift)
	}
	return res
}

func readString(data []byte, pos *int) string {
	length := int(readLEB128u(data, pos))
	str := string(data[*pos : *pos+length])
	*pos += length
	return str
}

type Transpiler struct {
	types           []WasmType
	numImports      uint32
	funcTypeIndices []uint32
	globals         []WasmGlobal
	exports         []WasmExport
	elementIndices  []uint32
	tableMin        uint32
	dataSegments    []WasmDataSegment
	codeList        [][]byte
	staticSet       map[uint32]bool
}

func main() {
	wasmPath := "runtime/go/hybridge/hyrt.wasm"
	outPath := "runtime/go/hybridge/hyrt_gen.go"

	if _, err := os.Stat(wasmPath); os.IsNotExist(err) {
		if _, err2 := os.Stat("hyrt.wasm"); err2 == nil {
			wasmPath = "hyrt.wasm"
			outPath = "hyrt_gen.go"
		}
	}

	if len(os.Args) > 1 {
		wasmPath = os.Args[1]
	}
	if len(os.Args) > 2 {
		outPath = os.Args[2]
	}

	data, err := os.ReadFile(wasmPath)
	if err != nil {
		fmt.Printf("Error reading wasm: %v\n", err)
		os.Exit(1)
	}

	tr := &Transpiler{}
	tr.parse(data)

	var out bytes.Buffer
	tr.emitGo(&out)

	if err := os.WriteFile(outPath, out.Bytes(), 0644); err != nil {
		fmt.Printf("Error writing generated Go file: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Successfully generated %s (%d bytes)\n", outPath, out.Len())
}

func (tr *Transpiler) buildCallTree(exportFnIdx uint32) []uint32 {
	var result []uint32
	visited := make(map[uint32]bool)

	var dfs func(fnIdx uint32)
	dfs = func(fnIdx uint32) {
		if visited[fnIdx] {
			return
		}
		visited[fnIdx] = true
		result = append(result, fnIdx)

		if fnIdx < tr.numImports {
			return
		}
		codeIdx := fnIdx - tr.numImports
		if int(codeIdx) >= len(tr.codeList) {
			return
		}
		code := tr.codeList[codeIdx]
		pos := 0
		localCntVec := readLEB128u(code, &pos)
		for l := uint32(0); l < localCntVec; l++ {
			_ = readLEB128u(code, &pos)
			pos++
		}
		for pos < len(code) {
			op := code[pos]
			pos++
			switch op {
			case 0x10: // call
				targetIdx := readLEB128u(code, &pos)
				dfs(targetIdx)
			case 0x11: // call_indirect
				_ = readLEB128u(code, &pos)
				pos++
			case 0x02, 0x03, 0x04:
				pos++
			case 0x0c, 0x0d:
				_ = readLEB128u(code, &pos)
			case 0x0e:
				cnt := readLEB128u(code, &pos)
				for i := uint32(0); i <= cnt; i++ {
					_ = readLEB128u(code, &pos)
				}
			case 0x20, 0x21, 0x22, 0x23, 0x24:
				_ = readLEB128u(code, &pos)
			case 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
				0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
				0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e:
				_ = readLEB128u(code, &pos)
				_ = readLEB128u(code, &pos)
			case 0x3f, 0x40:
				pos++
			case 0x41, 0x42:
				_ = readLEB128s(code, &pos)
			case 0x43:
				pos += 4
			case 0x44:
				pos += 8
			case 0xfc:
				sub := readLEB128u(code, &pos)
				if sub == 10 || sub == 11 {
					pos += 2
				}
			}
		}
	}

	dfs(exportFnIdx)
	fmt.Fprintf(os.Stderr, "buildCallTree(%d) reachable functions: %v\n", exportFnIdx, result)
	return result
}

func (tr *Transpiler) canStaticEmit(fnIdx uint32) (bool, byte) {
	if fnIdx < tr.numImports {
		return false, 0
	}
	codeIdx := fnIdx - tr.numImports
	if int(codeIdx) >= len(tr.codeList) {
		return false, 0
	}
	code := tr.codeList[codeIdx]
	pos := 0

	localCntVec := readLEB128u(code, &pos)
	for l := uint32(0); l < localCntVec; l++ {
		_ = readLEB128u(code, &pos)
		pos++ // valType
	}

	for pos < len(code) {
		op := code[pos]
		pos++
		switch op {
		case 0x01, 0x0b, 0x0f, 0x45, 0x46, 0x6a, 0x6b, 0x6c, 0x7c, 0xac, 0xad, 0xa7:
			// no immediates
		case 0x10: // call
			_ = readLEB128u(code, &pos)
		case 0x20, 0x21, 0x22: // local.get, local.set, local.tee
			_ = readLEB128u(code, &pos)
		case 0x41, 0x42: // i32.const, i64.const
			_ = readLEB128s(code, &pos)
		default:
			return false, op
		}
	}
	return true, 0
}

func (tr *Transpiler) emitStaticFn(fnIdx uint32, buf *bytes.Buffer) {
	if fnIdx < tr.numImports {
		return
	}
	codeIdx := fnIdx - tr.numImports
	if int(codeIdx) >= len(tr.codeList) {
		return
	}
	tIdx := tr.funcTypeIndices[codeIdx]
	wasmType := tr.types[tIdx]
	numParams := len(wasmType.Params)
	numResults := len(wasmType.Results)

	ok, fallbackOp := tr.canStaticEmit(fnIdx)
	if !ok {
		buf.WriteString(fmt.Sprintf("// fallback: opcode 0x%02x in genFn%d\n", fallbackOp, fnIdx))
		buf.WriteString(fmt.Sprintf("func genFn%d(", fnIdx))
		for i := 0; i < numParams; i++ {
			if i > 0 {
				buf.WriteString(", ")
			}
			buf.WriteString(fmt.Sprintf("p%d uint64", i))
		}
		buf.WriteString(") uint64 {\n")
		buf.WriteString(fmt.Sprintf("\treturn execWasm(%d", fnIdx))
		for i := 0; i < numParams; i++ {
			buf.WriteString(fmt.Sprintf(", p%d", i))
		}
		buf.WriteString(")\n}\n\n")
		return
	}

	code := tr.codeList[codeIdx]
	pos := 0

	localCntVec := readLEB128u(code, &pos)
	extraLocals := 0
	for l := uint32(0); l < localCntVec; l++ {
		cnt := readLEB128u(code, &pos)
		pos++ // valType
		extraLocals += int(cnt)
	}

	buf.WriteString(fmt.Sprintf("func genFn%d(", fnIdx))
	for i := 0; i < numParams; i++ {
		if i > 0 {
			buf.WriteString(", ")
		}
		buf.WriteString(fmt.Sprintf("p%d uint64", i))
	}
	buf.WriteString(") uint64 {\n")

	if extraLocals > 0 {
		buf.WriteString("\tvar ")
		for i := 0; i < extraLocals; i++ {
			if i > 0 {
				buf.WriteString(", ")
			}
			buf.WriteString(fmt.Sprintf("l%d", numParams+i))
		}
		buf.WriteString(" uint64\n")
	}

	buf.WriteString("\tvar s [32]uint64\n")
	buf.WriteString("\t_ = s\n")
	buf.WriteString("\tsp := 0\n")
	buf.WriteString("\t_ = sp\n")

	for pos < len(code) {
		op := code[pos]
		pos++

		switch op {
		case 0x01: // nop
			buf.WriteString("\t// nop\n")
		case 0x0b: // end
			// emit nothing for end
		case 0x0f: // return
			if numResults > 0 {
				buf.WriteString("\treturn s[sp-1]\n")
			} else {
				buf.WriteString("\treturn 0\n")
			}
		case 0x10: // call
			targetIdx := readLEB128u(code, &pos)
			pCnt := 0
			totalFuncs := int(tr.numImports) + len(tr.funcTypeIndices)
			if int(targetIdx) < totalFuncs {
				t := tr.types[tr.funcTypeIndices[targetIdx-tr.numImports]]
				pCnt = len(t.Params)
			}
			var callArgs []string
			for i := 0; i < pCnt; i++ {
				callArgs = append(callArgs, fmt.Sprintf("s[sp-%d]", pCnt-i))
			}
			argList := ""
			for i, a := range callArgs {
				if i > 0 {
					argList += ", "
				}
				argList += a
			}
			rCnt := 0
			if int(targetIdx) < totalFuncs {
				t := tr.types[tr.funcTypeIndices[targetIdx-tr.numImports]]
				rCnt = len(t.Results)
			}
			if tr.staticSet[targetIdx] {
				targetName := fmt.Sprintf("genFn%d", targetIdx)
				if rCnt > 0 {
					if pCnt > 0 {
						buf.WriteString(fmt.Sprintf("\ts[sp-%d] = %s(%s)\n", pCnt, targetName, argList))
						if pCnt > 1 {
							buf.WriteString(fmt.Sprintf("\tsp -= %d\n", pCnt-1))
						}
					} else {
						buf.WriteString(fmt.Sprintf("\ts[sp] = %s(); sp++\n", targetName))
					}
				} else {
					buf.WriteString(fmt.Sprintf("\t%s(%s)\n", targetName, argList))
					if pCnt > 0 {
						buf.WriteString(fmt.Sprintf("\tsp -= %d\n", pCnt))
					}
				}
			} else {
				targetName := fmt.Sprintf("execWasm(%d", targetIdx)
				if argList != "" {
					argList = ", " + argList
				}
				if rCnt > 0 {
					if pCnt > 0 {
						buf.WriteString(fmt.Sprintf("\ts[sp-%d] = %s%s)\n", pCnt, targetName, argList))
						if pCnt > 1 {
							buf.WriteString(fmt.Sprintf("\tsp -= %d\n", pCnt-1))
						}
					} else {
						buf.WriteString(fmt.Sprintf("\ts[sp] = %s%s); sp++\n", targetName, argList))
					}
				} else {
					buf.WriteString(fmt.Sprintf("\t%s%s)\n", targetName, argList))
					if pCnt > 0 {
						buf.WriteString(fmt.Sprintf("\tsp -= %d\n", pCnt))
					}
				}
			}
		case 0x20: // local.get
			idx := readLEB128u(code, &pos)
			var varName string
			if int(idx) < numParams {
				varName = fmt.Sprintf("p%d", idx)
			} else {
				varName = fmt.Sprintf("l%d", idx)
			}
			buf.WriteString(fmt.Sprintf("\ts[sp] = %s; sp++\n", varName))
		case 0x21: // local.set
			idx := readLEB128u(code, &pos)
			var varName string
			if int(idx) < numParams {
				varName = fmt.Sprintf("p%d", idx)
			} else {
				varName = fmt.Sprintf("l%d", idx)
			}
			buf.WriteString(fmt.Sprintf("\tsp--; %s = s[sp]\n", varName))
		case 0x22: // local.tee
			idx := readLEB128u(code, &pos)
			var varName string
			if int(idx) < numParams {
				varName = fmt.Sprintf("p%d", idx)
			} else {
				varName = fmt.Sprintf("l%d", idx)
			}
			buf.WriteString(fmt.Sprintf("\t%s = s[sp-1]\n", varName))
		case 0x41: // i32.const
			v := readLEB128s(code, &pos)
			buf.WriteString(fmt.Sprintf("\ts[sp] = uint64(uint32(%d)); sp++\n", v))
		case 0x42: // i64.const
			v := readLEB128s(code, &pos)
			buf.WriteString(fmt.Sprintf("\ts[sp] = uint64(%d); sp++\n", v))
		case 0x45: // i32.eqz
			buf.WriteString("\tsp--; if uint32(s[sp]) == 0 { s[sp] = 1 } else { s[sp] = 0 }; sp++\n")
		case 0x46: // i32.eq
			buf.WriteString("\tsp -= 2; if uint32(s[sp]) == uint32(s[sp+1]) { s[sp] = 1 } else { s[sp] = 0 }; sp++\n")
		case 0x6a: // i32.add
			buf.WriteString("\tsp -= 2; s[sp] = uint64(uint32(s[sp]) + uint32(s[sp+1])); sp++\n")
		case 0x6b: // i32.sub
			buf.WriteString("\tsp -= 2; s[sp] = uint64(uint32(s[sp]) - uint32(s[sp+1])); sp++\n")
		case 0x6c: // i32.mul
			buf.WriteString("\tsp -= 2; s[sp] = uint64(uint32(s[sp]) * uint32(s[sp+1])); sp++\n")
		case 0x7c: // i64.add
			buf.WriteString("\tsp -= 2; s[sp] = s[sp] + s[sp+1]; sp++\n")
		case 0xac: // i64.extend_i32_s
			buf.WriteString("\tsp--; s[sp] = uint64(int64(int32(s[sp]))); sp++\n")
		case 0xad: // i64.extend_i32_u
			buf.WriteString("\tsp--; s[sp] = uint64(uint32(s[sp])); sp++\n")
		case 0xa7: // i32.wrap_i64
			buf.WriteString("\tsp--; s[sp] = uint64(uint32(s[sp])); sp++\n")
		}
	}

	if numResults > 0 {
		buf.WriteString("\treturn s[sp-1]\n")
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")
}

func (tr *Transpiler) parse(data []byte) {
	pos := 8
	for pos < len(data) {
		secID := data[pos]
		pos++
		secSize := int(readLEB128u(data, &pos))
		secEnd := pos + secSize

		switch secID {
		case 1: // Type section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				form := data[pos]
				pos++
				if form == 0x60 {
					pCnt := readLEB128u(data, &pos)
					params := make([]byte, pCnt)
					copy(params, data[pos:pos+int(pCnt)])
					pos += int(pCnt)

					rCnt := readLEB128u(data, &pos)
					rets := make([]byte, rCnt)
					copy(rets, data[pos:pos+int(rCnt)])
					pos += int(rCnt)

					tr.types = append(tr.types, WasmType{Params: params, Results: rets})
				}
			}
		case 2: // Import section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				_ = readString(data, &pos) // module
				_ = readString(data, &pos) // field
				kind := data[pos]
				pos++
				if kind == 0 { // Func
					_ = readLEB128u(data, &pos)
					tr.numImports++
				} else if kind == 1 { // Table
					pos++
					flag := data[pos]; pos++
					_ = readLEB128u(data, &pos)
					if flag&1 != 0 { _ = readLEB128u(data, &pos) }
				} else if kind == 2 { // Memory
					flag := data[pos]; pos++
					_ = readLEB128u(data, &pos)
					if flag&1 != 0 { _ = readLEB128u(data, &pos) }
				} else if kind == 3 { // Global
					pos += 2
				}
			}
		case 3: // Function section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				tIdx := readLEB128u(data, &pos)
				tr.funcTypeIndices = append(tr.funcTypeIndices, tIdx)
			}
		case 4: // Table section
			cnt := readLEB128u(data, &pos)
			if cnt > 0 {
				_ = data[pos] // 0x70
				pos++
				flag := data[pos]
				pos++
				tr.tableMin = readLEB128u(data, &pos)
				if flag&1 != 0 {
					_ = readLEB128u(data, &pos)
				}
			}
		case 6: // Global section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				valType := data[pos]
				pos++
				mut := data[pos] == 1
				pos++
				// expr
				op := data[pos]
				pos++
				var initVal int64
				if op == 0x41 || op == 0x42 { // const
					initVal = readLEB128s(data, &pos)
				}
				pos++ // 0x0b (end)
				tr.globals = append(tr.globals, WasmGlobal{Type: valType, Mutable: mut, InitVal: initVal})
			}
		case 7: // Export section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				name := readString(data, &pos)
				kind := data[pos]
				pos++
				idx := readLEB128u(data, &pos)
				tr.exports = append(tr.exports, WasmExport{Name: name, Kind: kind, Index: idx})
			}
		case 9: // Element section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				mode := readLEB128u(data, &pos)
				if mode == 0 {
					pos++ // 0x41
					_ = readLEB128s(data, &pos)
					pos++ // 0x0b
					elemCnt := readLEB128u(data, &pos)
					for e := uint32(0); e < elemCnt; e++ {
						fIdx := readLEB128u(data, &pos)
						tr.elementIndices = append(tr.elementIndices, fIdx)
					}
				}
			}
		case 10: // Code section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				bodySize := readLEB128u(data, &pos)
				bodyEnd := pos + int(bodySize)
				tr.codeList = append(tr.codeList, data[pos:bodyEnd])
				pos = bodyEnd
			}
		case 11: // Data section
			cnt := readLEB128u(data, &pos)
			for i := uint32(0); i < cnt; i++ {
				mode := readLEB128u(data, &pos)
				if mode == 0 {
					pos++ // 0x41 (i32.const)
					off := uint32(readLEB128s(data, &pos))
					pos++ // 0x0b (end)
					dLen := readLEB128u(data, &pos)
					segData := make([]byte, dLen)
					copy(segData, data[pos:pos+int(dLen)])
					pos += int(dLen)
					tr.dataSegments = append(tr.dataSegments, WasmDataSegment{Offset: off, Data: segData})
				}
			}
		}
		pos = secEnd
	}
}

func (tr *Transpiler) emitGo(buf *bytes.Buffer) {
	buf.WriteString("// Code generated by tools/wasm2go; DO NOT EDIT.\n")
	buf.WriteString("package hybridge\n\n")
	buf.WriteString("import (\n")
	buf.WriteString("\t\"encoding/binary\"\n")
	buf.WriteString("\t\"fmt\"\n")
	buf.WriteString("\t\"math\"\n")
	buf.WriteString("\t\"math/bits\"\n")
	buf.WriteString("\t\"sync\"\n")
	buf.WriteString("\t\"github.com/tetratelabs/wazero/api\"\n")
	buf.WriteString(")\n\n")

	// Global linear memory array (16 MB initial capacity)
	buf.WriteString("var mem = make([]byte, 16777216)\n\n")

	// Bug 2: Shadow stack pointer initialization (Global 0)
	stackPtrInit := uint64(65536)
	for _, seg := range tr.dataSegments {
		end := uint64(seg.Offset) + uint64(len(seg.Data))
		if end > stackPtrInit {
			stackPtrInit = end
		}
	}
	if stackPtrInit%16 != 0 {
		stackPtrInit = (stackPtrInit + 15) &^ 15
	}
	stackPtrInit += 65536

	// Globals
	buf.WriteString(fmt.Sprintf("var wasmGlobals = [%d]uint64{\n", len(tr.globals)))
	for i, g := range tr.globals {
		val := uint64(g.InitVal)
		if i == 0 {
			val = stackPtrInit
		}
		buf.WriteString(fmt.Sprintf("\t%d,\n", val))
	}
	buf.WriteString("}\n\n")

	buf.WriteString(fmt.Sprintf("const wasmNumImports = %d\n\n", tr.numImports))

	// Function Table
	tableSize := tr.tableMin
	if uint32(len(tr.elementIndices)) > tableSize {
		tableSize = uint32(len(tr.elementIndices))
	}
	if tableSize < 1 {
		tableSize = 1
	}

	buf.WriteString(fmt.Sprintf("var funcTable = [%d]uint32{\n", tableSize))
	for i, fIdx := range tr.elementIndices {
		buf.WriteString(fmt.Sprintf("\t%d: %d,\n", i, fIdx))
	}
	buf.WriteString("}\n\n")

	// Code byte slices array
	buf.WriteString(fmt.Sprintf("var wasmCode = [%d][]byte{\n", len(tr.codeList)))
	for _, code := range tr.codeList {
		buf.WriteString("\t{")
		for _, b := range code {
			buf.WriteString(fmt.Sprintf("0x%02x, ", b))
		}
		buf.WriteString("},\n")
	}
	buf.WriteString("}\n\n")

	// Function Parameter Counts
	totalFuncs := int(tr.numImports) + len(tr.funcTypeIndices)
	buf.WriteString(fmt.Sprintf("var wasmFuncParamCnt = [%d]int{\n", totalFuncs))
	for _, tIdx := range tr.funcTypeIndices {
		t := tr.types[tIdx]
		buf.WriteString(fmt.Sprintf("\t%d,\n", len(t.Params)))
	}
	buf.WriteString("}\n\n")

	// Function Result Counts
	buf.WriteString(fmt.Sprintf("var wasmFuncRetCnt = [%d]int{\n", totalFuncs))
	for _, tIdx := range tr.funcTypeIndices {
		t := tr.types[tIdx]
		buf.WriteString(fmt.Sprintf("\t%d,\n", len(t.Results)))
	}
	buf.WriteString("}\n\n")

	// init() block initializing Data section segments into memory
	buf.WriteString("func init() {\n")
	for _, seg := range tr.dataSegments {
		buf.WriteString(fmt.Sprintf("\tcopy(mem[%d:], []byte{", seg.Offset))
		for _, b := range seg.Data {
			buf.WriteString(fmt.Sprintf("0x%02x, ", b))
		}
		buf.WriteString("})\n")
	}
	buf.WriteString("}\n\n")

	// Discover and statically emit call trees for target exports
	exportMap := make(map[string]uint32)
	for _, exp := range tr.exports {
		if exp.Kind == 0 { // Func
			exportMap[exp.Name] = exp.Index
		}
	}
	if tr.staticSet == nil {
		tr.staticSet = make(map[uint32]bool)
	}
	targetExports := []string{"hy_nop", "hyrt_abi_word", "hyrt_stack_alignment_probe", "hyrt_alloc", "hyrt_free"}
	var allTreeFuncs []uint32
	for _, expName := range targetExports {
		if fIdx, ok := exportMap[expName]; ok {
			tree := tr.buildCallTree(fIdx)
			for _, f := range tree {
				if !tr.staticSet[f] {
					tr.staticSet[f] = true
					allTreeFuncs = append(allTreeFuncs, f)
				}
			}
		}
	}
	for _, f := range allTreeFuncs {
		tr.emitStaticFn(f, buf)
	}

	// VM Exec engine
	tr.emitVMExec(buf)

	// Memory Wrapper
	tr.emitMemoryWrapper(buf)

	// Export Wrappers
	tr.emitExportWrappers(buf)
}

func (tr *Transpiler) emitVMExec(buf *bytes.Buffer) {
	buf.WriteString(`var vmStackPool = sync.Pool{
	New: func() any {
		s := make([]uint64, 1024)
		return &s
	},
}

func readLEB128uGen(data []byte, pos *int) uint32 {
	var res uint32
	var shift uint
	for {
		b := data[*pos]
		*pos++
		res |= uint32(b&0x7f) << shift
		if b&0x80 == 0 {
			break
		}
		shift += 7
	}
	return res
}

func readLEB128sGen(data []byte, pos *int) int64 {
	var res int64
	var shift uint
	var b byte
	for {
		b = data[*pos]
		*pos++
		res |= int64(b&0x7f) << shift
		shift += 7
		if b&0x80 == 0 {
			break
		}
	}
	if shift < 64 && (b&0x40) != 0 {
		res |= -(1 << shift)
	}
	return res
}

func skipOpcode(code []byte, pos *int) {
	op := code[*pos]
	*pos++
	switch op {
	case 0x02, 0x03, 0x04: // block, loop, if
		*pos++ // blocktype
	case 0x0c, 0x0d: // br, br_if
		_ = readLEB128uGen(code, pos)
	case 0x0e: // br_table
		cnt := readLEB128uGen(code, pos)
		for i := uint32(0); i <= cnt; i++ {
			_ = readLEB128uGen(code, pos)
		}
	case 0x10: // call
		_ = readLEB128uGen(code, pos)
	case 0x11: // call_indirect
		_ = readLEB128uGen(code, pos)
		*pos++ // tableidx
	case 0x20, 0x21, 0x22: // local.get, local.set, local.tee
		_ = readLEB128uGen(code, pos)
	case 0x23, 0x24: // global.get, global.set
		_ = readLEB128uGen(code, pos)
	case 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
		0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37,
		0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e: // memory load/store
		_ = readLEB128uGen(code, pos) // align
		_ = readLEB128uGen(code, pos) // offset
	case 0x3f, 0x40: // memory.size, memory.grow
		*pos++
	case 0x41: // i32.const
		_ = readLEB128sGen(code, pos)
	case 0x42: // i64.const
		_ = readLEB128sGen(code, pos)
	case 0x43: // f32.const
		*pos += 4
	case 0x44: // f64.const
		*pos += 8
	case 0xfc: // saturating float or memory operations
		sub := readLEB128uGen(code, pos)
		if sub == 10 || sub == 11 { // memory.fill, memory.copy
			*pos += 2
		}
	}
}

func ensureMem(needed int) {
	if needed > len(mem) {
		newCap := len(mem) * 2
		if newCap < needed {
			newCap = needed + 65536*16
		}
		newMem := make([]byte, newCap)
		copy(newMem, mem)
		mem = newMem
	}
}

func execWasm(fnIdx uint32, args ...uint64) uint64 {
	codeIdx := fnIdx - wasmNumImports
	if int(codeIdx) >= len(wasmCode) {
		return 0
	}
	code := wasmCode[codeIdx]
	pos := 0

	// Parse local counts
	localCntVec := readLEB128uGen(code, &pos)
	totalLocals := wasmFuncParamCnt[fnIdx]
	for l := uint32(0); l < localCntVec; l++ {
		count := readLEB128uGen(code, &pos)
		pos++ // valType
		totalLocals += int(count)
	}

	var locals [512]uint64
	copy(locals[:], args)

	spPtr := vmStackPool.Get().(*[]uint64)
	valStack := *spPtr
	sp := 0

	type blockFrame struct {
		kind byte // 2=block, 3=loop, 4=if, 5=else
		startPos int
		stackLen int
	}
	var controlStack [64]blockFrame
	csp := 0

	// Outer function block
	controlStack[0] = blockFrame{kind: 2, startPos: pos, stackLen: 0}
	csp = 1

	for pos < len(code) {
		op := code[pos]
		pos++

		switch op {
		case 0x00: // Bug 1 fix: 0x00 is unreachable
			panic("wasm unreachable executed")
		case 0x01: // Bug 1 fix: 0x01 is nop
			// do nothing
		case 0x02, 0x03, 0x04: // block, loop, if
			pos++ // blocktype
			controlStack[csp] = blockFrame{kind: op, startPos: pos, stackLen: sp}
			csp++
			if op == 0x04 { // if
				sp--
				cond := valStack[sp]
				if uint32(cond) == 0 {
					// Skip to else or end
					depth := 1
					for pos < len(code) && depth > 0 {
						o := code[pos]
						if o == 0x02 || o == 0x03 || o == 0x04 {
							depth++
						} else if o == 0x05 {
							if depth == 1 {
								depth = 0
								skipOpcode(code, &pos)
								break
							}
						} else if o == 0x0b {
							depth--
							if depth == 0 {
								skipOpcode(code, &pos)
								break
							}
						}
						skipOpcode(code, &pos)
					}
				}
			}
		case 0x05: // else
			// Skip to end of if/else
			depth := 1
			for pos < len(code) && depth > 0 {
				o := code[pos]
				if o == 0x02 || o == 0x03 || o == 0x04 {
					depth++
				} else if o == 0x0b {
					depth--
					if depth == 0 {
						skipOpcode(code, &pos)
						break
					}
				}
				skipOpcode(code, &pos)
			}
		case 0x0b: // end
			if csp > 0 {
				csp--
			}
		case 0x0c: // br (with stack unwinding)
			depth := int(readLEB128uGen(code, &pos))
			if depth < csp {
				target := controlStack[csp-1-depth]
				var topVal uint64
				hasVal := false
				if sp > target.stackLen {
					topVal = valStack[sp-1]
					hasVal = true
				}
				sp = target.stackLen
				if hasVal {
					valStack[sp] = topVal
					sp++
				}
				if target.kind == 0x03 { // loop
					pos = target.startPos
				} else {
					// Skip to end of target block
					d := depth + 1
					for pos < len(code) && d > 0 {
						o := code[pos]
						if o == 0x02 || o == 0x03 || o == 0x04 {
							d++
						} else if o == 0x0b {
							d--
							if d == 0 {
								skipOpcode(code, &pos)
								break
							}
						}
						skipOpcode(code, &pos)
					}
				}
			} else {
				vmStackPool.Put(spPtr)
				if sp > 0 { return valStack[sp-1] }
				return 0
			}
		case 0x0d: // br_if (with stack unwinding)
			depth := int(readLEB128uGen(code, &pos))
			sp--
			cond := valStack[sp]
			if cond != 0 {
				if depth < csp {
					target := controlStack[csp-1-depth]
					var topVal uint64
					hasVal := false
					if sp > target.stackLen {
						topVal = valStack[sp-1]
						hasVal = true
					}
					sp = target.stackLen
					if hasVal {
						valStack[sp] = topVal
						sp++
					}
					if target.kind == 0x03 {
						pos = target.startPos
					} else {
						d := depth + 1
						for pos < len(code) && d > 0 {
							o := code[pos]
							if o == 0x02 || o == 0x03 || o == 0x04 {
								d++
							} else if o == 0x0b {
								d--
								if d == 0 {
									skipOpcode(code, &pos)
									break
								}
							}
							skipOpcode(code, &pos)
						}
					}
				} else {
					vmStackPool.Put(spPtr)
					if sp > 0 { return valStack[sp-1] }
					return 0
				}
			}
		case 0x0e: // br_table (with stack unwinding)
			cnt := int(readLEB128uGen(code, &pos))
			var targets []int
			for i := 0; i < cnt; i++ {
				targets = append(targets, int(readLEB128uGen(code, &pos)))
			}
			defTarget := int(readLEB128uGen(code, &pos))
			sp--
			idx := int(valStack[sp])
			chosenTarget := defTarget
			if idx >= 0 && idx < len(targets) {
				chosenTarget = targets[idx]
			}
			if chosenTarget < csp {
				target := controlStack[csp-1-chosenTarget]
				var topVal uint64
				hasVal := false
				if sp > target.stackLen {
					topVal = valStack[sp-1]
					hasVal = true
				}
				sp = target.stackLen
				if hasVal {
					valStack[sp] = topVal
					sp++
				}
				if target.kind == 0x03 {
					pos = target.startPos
				} else {
					d := chosenTarget + 1
					for pos < len(code) && d > 0 {
						o := code[pos]
						if o == 0x02 || o == 0x03 || o == 0x04 {
							d++
						} else if o == 0x0b {
							d--
							if d == 0 {
								skipOpcode(code, &pos)
								break
							}
						}
						skipOpcode(code, &pos)
					}
				}
			}
		case 0x0f: // return
			vmStackPool.Put(spPtr)
			if sp > 0 { return valStack[sp-1] }
			return 0
		case 0x10: // call (Bug 4 fix: zero alloc call args)
			targetIdx := readLEB128uGen(code, &pos)
			pCnt := wasmFuncParamCnt[targetIdx]
			var callArgsBuf [16]uint64
			callArgs := callArgsBuf[:pCnt]
			for i := pCnt - 1; i >= 0; i-- {
				sp--
				callArgs[i] = valStack[sp]
			}
			res := execWasm(targetIdx, callArgs...)
			if wasmFuncRetCnt[targetIdx] > 0 {
				valStack[sp] = res
				sp++
			}
		case 0x11: // call_indirect (Bug 4 fix: zero alloc call args)
			_ = readLEB128uGen(code, &pos)
			pos++ // tableIdx
			sp--
			tblIdx := valStack[sp]
			targetIdx := funcTable[tblIdx]
			pCnt := wasmFuncParamCnt[targetIdx]
			var callArgsBuf [16]uint64
			callArgs := callArgsBuf[:pCnt]
			for i := pCnt - 1; i >= 0; i-- {
				sp--
				callArgs[i] = valStack[sp]
			}
			res := execWasm(targetIdx, callArgs...)
			if wasmFuncRetCnt[targetIdx] > 0 {
				valStack[sp] = res
				sp++
			}
		case 0x1a: // drop
			sp--
		case 0x1b: // select
			sp--
			c := valStack[sp]
			sp--
			val2 := valStack[sp]
			sp--
			val1 := valStack[sp]
			if c != 0 { valStack[sp] = val1 } else { valStack[sp] = val2 }
			sp++
		case 0x20: // local.get
			idx := readLEB128uGen(code, &pos)
			valStack[sp] = locals[idx]
			sp++
		case 0x21: // local.set
			idx := readLEB128uGen(code, &pos)
			sp--
			locals[idx] = valStack[sp]
		case 0x22: // local.tee
			idx := readLEB128uGen(code, &pos)
			locals[idx] = valStack[sp-1]
		case 0x23: // global.get
			idx := readLEB128uGen(code, &pos)
			valStack[sp] = wasmGlobals[idx]
			sp++
		case 0x24: // global.set
			idx := readLEB128uGen(code, &pos)
			sp--
			wasmGlobals[idx] = valStack[sp]
		case 0x28: // i32.load
			pos++ // align
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 4)
			valStack[sp] = uint64(binary.LittleEndian.Uint32(mem[addr:]))
			sp++
		case 0x29: // i64.load
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 8)
			valStack[sp] = binary.LittleEndian.Uint64(mem[addr:])
			sp++
		case 0x2c: // i32.load8_u
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 1)
			valStack[sp] = uint64(mem[addr])
			sp++
		case 0x2d: // i32.load8_s
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 1)
			valStack[sp] = uint64(uint32(int8(mem[addr])))
			sp++
		case 0x2e: // i32.load16_s
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 2)
			valStack[sp] = uint64(uint32(int16(binary.LittleEndian.Uint16(mem[addr:]))))
			sp++
		case 0x2f: // i32.load16_u
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 2)
			valStack[sp] = uint64(binary.LittleEndian.Uint16(mem[addr:]))
			sp++
		case 0x36: // i32.store
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			v := valStack[sp]
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 4)
			binary.LittleEndian.PutUint32(mem[addr:], uint32(v))
		case 0x37: // i64.store
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			v := valStack[sp]
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 8)
			binary.LittleEndian.PutUint64(mem[addr:], v)
		case 0x3a: // i32.store8
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			v := valStack[sp]
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 1)
			mem[addr] = byte(v)
		case 0x3b: // i32.store16
			pos++
			off := readLEB128uGen(code, &pos)
			sp--
			v := valStack[sp]
			sp--
			ptr := valStack[sp]
			addr := int(uint32(ptr) + off)
			ensureMem(addr + 2)
			binary.LittleEndian.PutUint16(mem[addr:], uint16(v))
		case 0x3f: // memory.size
			pos++
			valStack[sp] = uint64(len(mem) / 65536)
			sp++
		case 0x40: // memory.grow
			pos++
			sp--
			valStack[sp] = uint64(len(mem) / 65536)
			sp++
		case 0x41: // i32.const
			v := readLEB128sGen(code, &pos)
			valStack[sp] = uint64(uint32(v))
			sp++
		case 0x42: // i64.const
			v := readLEB128sGen(code, &pos)
			valStack[sp] = uint64(v)
			sp++
		case 0x45: // i32.eqz
			sp--
			a := valStack[sp]
			if uint32(a) == 0 { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x46: // i32.eq
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) == uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x47: // i32.ne
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) != uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x48: // i32.lt_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int32(a) < int32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x49: // i32.lt_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) < uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4a: // i32.gt_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int32(a) > int32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4b: // i32.gt_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) > uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4c: // i32.le_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int32(a) <= int32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4d: // i32.le_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) <= uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4e: // i32.ge_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int32(a) >= int32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x4f: // i32.ge_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if uint32(a) >= uint32(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x50: // i64.eqz
			sp--
			a := valStack[sp]
			if a == 0 { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x51: // i64.eq
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a == b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x52: // i64.ne
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a != b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x53: // i64.lt_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int64(a) < int64(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x54: // i64.lt_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a < b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x55: // i64.gt_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int64(a) > int64(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x56: // i64.gt_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a > b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x57: // Bug 3 fix: i64.ge_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int64(a) >= int64(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x58: // Bug 3 fix: i64.ge_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a >= b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x59: // Bug 3 fix: i64.le_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if int64(a) <= int64(b) { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x5a: // Bug 3 fix: i64.le_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			if a <= b { valStack[sp] = 1 } else { valStack[sp] = 0 }
			sp++
		case 0x67: // i32.clz
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.LeadingZeros32(uint32(a)))
			sp++
		case 0x68: // i32.ctz
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.TrailingZeros32(uint32(a)))
			sp++
		case 0x69: // i32.popcnt
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.OnesCount32(uint32(a)))
			sp++
		case 0x6a: // i32.add
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) + uint32(b))
			sp++
		case 0x6b: // i32.sub
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) - uint32(b))
			sp++
		case 0x6c: // i32.mul
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) * uint32(b))
			sp++
		case 0x6d: // i32.div_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(int32(a) / int32(b)))
			sp++
		case 0x6e: // i32.div_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) / uint32(b))
			sp++
		case 0x6f: // Bug 3 fix: i32.rem_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(int32(a) % int32(b)))
			sp++
		case 0x70: // Bug 3 fix: i32.rem_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) % uint32(b))
			sp++
		case 0x71: // i32.and
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) & uint32(b))
			sp++
		case 0x72: // i32.or
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) | uint32(b))
			sp++
		case 0x73: // i32.xor
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) ^ uint32(b))
			sp++
		case 0x74: // i32.shl
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) << (uint32(b) & 31))
			sp++
		case 0x75: // i32.shr_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(int32(a) >> (uint32(b) & 31)))
			sp++
		case 0x76: // i32.shr_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a) >> (uint32(b) & 31))
			sp++
		case 0x77: // Bug 3 fix: i32.rotl
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.RotateLeft32(uint32(a), int(b)))
			sp++
		case 0x78: // Bug 3 fix: i32.rotr
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.RotateLeft32(uint32(a), -int(b)))
			sp++
		case 0x79: // i64.clz
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.LeadingZeros64(a))
			sp++
		case 0x7a: // i64.ctz
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.TrailingZeros64(a))
			sp++
		case 0x7b: // i64.popcnt
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(bits.OnesCount64(a))
			sp++
		case 0x7c: // i64.add
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a + b
			sp++
		case 0x7d: // i64.sub
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a - b
			sp++
		case 0x7e: // i64.mul
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a * b
			sp++
		case 0x7f: // i64.div_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(int64(a) / int64(b))
			sp++
		case 0x80: // i64.div_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a / b
			sp++
		case 0x81: // Bug 3 fix: i64.rem_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(int64(a) % int64(b))
			sp++
		case 0x82: // Bug 3 fix: i64.rem_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a % b
			sp++
		case 0x83: // i64.and
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a & b
			sp++
		case 0x84: // i64.or
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a | b
			sp++
		case 0x85: // i64.xor
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a ^ b
			sp++
		case 0x86: // i64.shl
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a << (b & 63)
			sp++
		case 0x87: // i64.shr_s
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(int64(a) >> (b & 63))
			sp++
		case 0x88: // i64.shr_u
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = a >> (b & 63)
			sp++
		case 0x89: // Bug 3 fix: i64.rotl
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = bits.RotateLeft64(a, int(b))
			sp++
		case 0x8a: // Bug 3 fix: i64.rotr
			sp--
			b := valStack[sp]
			sp--
			a := valStack[sp]
			valStack[sp] = bits.RotateLeft64(a, -int(b))
			sp++
		case 0xa7: // i32.wrap_i64
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a))
			sp++
		case 0xac: // i64.extend_i32_s
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(int64(int32(a)))
			sp++
		case 0xad: // i64.extend_i32_u
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(uint32(a))
			sp++
		case 0xb7: // Bug 3 fix: f32.convert_i32_s
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(math.Float32bits(float32(int32(a))))
			sp++
		case 0xb8: // Bug 3 fix: f32.convert_i32_u
			sp--
			a := valStack[sp]
			valStack[sp] = uint64(math.Float32bits(float32(uint32(a))))
			sp++
		case 0xba: // f64.convert_i32_u
			sp--
			a := valStack[sp]
			valStack[sp] = math.Float64bits(float64(uint32(a)))
			sp++
		case 0xbd: // f64.convert_i64_u
			sp--
			a := valStack[sp]
			valStack[sp] = math.Float64bits(float64(a))
			sp++
		case 0xc1: // f64.promote_f32
			sp--
			a := valStack[sp]
			valStack[sp] = math.Float64bits(float64(math.Float32frombits(uint32(a))))
			sp++
		case 0xfc: // saturating float or memory operations
			sub := readLEB128uGen(code, &pos)
			if sub == 10 { // memory.fill
				pos += 2
				sp--
				n := valStack[sp]
				sp--
				val := valStack[sp]
				sp--
				ptr := valStack[sp]
				addr := int(uint32(ptr))
				ensureMem(addr + int(uint32(n)))
				for i := uint32(0); i < uint32(n); i++ {
					mem[addr+int(i)] = byte(val)
				}
			} else if sub == 11 { // memory.copy
				pos += 2
				sp--
				n := valStack[sp]
				sp--
				src := valStack[sp]
				sp--
				dst := valStack[sp]
				dstAddr := int(uint32(dst))
				srcAddr := int(uint32(src))
				count := int(uint32(n))
				ensureMem(dstAddr + count)
				ensureMem(srcAddr + count)
				copy(mem[dstAddr:dstAddr+count], mem[srcAddr:srcAddr+count])
			}
		default: // Bug 3 fix: panic on unimplemented opcode
			panic(fmt.Sprintf("unimplemented opcode 0x%02x at pos %d in fn %d", op, pos-1, fnIdx))
		}
	}

	vmStackPool.Put(spPtr)
	if sp > 0 {
		return valStack[sp-1]
	}
	return 0
}
`)
}

func (tr *Transpiler) emitMemoryWrapper(buf *bytes.Buffer) {
	buf.WriteString("type genMemory struct{ api.Memory }\n\n")

	buf.WriteString("func (m genMemory) Read(offset uint32, byteCount uint32) ([]byte, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+int(byteCount))\n")
	buf.WriteString("\tres := make([]byte, byteCount)\n")
	buf.WriteString("\tcopy(res, mem[offset:offset+byteCount])\n")
	buf.WriteString("\treturn res, true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadByte(offset uint32) (byte, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+1)\n")
	buf.WriteString("\treturn mem[offset], true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadUint16Le(offset uint32) (uint16, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+2)\n")
	buf.WriteString("\treturn binary.LittleEndian.Uint16(mem[offset:]), true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadUint32Le(offset uint32) (uint32, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+4)\n")
	buf.WriteString("\treturn binary.LittleEndian.Uint32(mem[offset:]), true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadUint64Le(offset uint32) (uint64, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+8)\n")
	buf.WriteString("\treturn binary.LittleEndian.Uint64(mem[offset:]), true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadFloat32Le(offset uint32) (float32, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+4)\n")
	buf.WriteString("\treturn math.Float32frombits(binary.LittleEndian.Uint32(mem[offset:])), true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) ReadFloat64Le(offset uint32) (float64, bool) {\n")
	buf.WriteString("\tensureMem(int(offset)+8)\n")
	buf.WriteString("\treturn math.Float64frombits(binary.LittleEndian.Uint64(mem[offset:])), true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) Write(offset uint32, data []byte) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+len(data))\n")
	buf.WriteString("\tcopy(mem[offset:], data)\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteByte(offset uint32, val byte) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+1)\n")
	buf.WriteString("\tmem[offset] = val\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteUint16Le(offset uint32, val uint16) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+2)\n")
	buf.WriteString("\tbinary.LittleEndian.PutUint16(mem[offset:], val)\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteUint32Le(offset uint32, val uint32) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+4)\n")
	buf.WriteString("\tbinary.LittleEndian.PutUint32(mem[offset:], val)\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteUint64Le(offset uint32, val uint64) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+8)\n")
	buf.WriteString("\tbinary.LittleEndian.PutUint64(mem[offset:], val)\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteFloat32Le(offset uint32, val float32) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+4)\n")
	buf.WriteString("\tbinary.LittleEndian.PutUint32(mem[offset:], math.Float32bits(val))\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteFloat64Le(offset uint32, val float64) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+8)\n")
	buf.WriteString("\tbinary.LittleEndian.PutUint64(mem[offset:], math.Float64bits(val))\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) WriteString(offset uint32, val string) bool {\n")
	buf.WriteString("\tensureMem(int(offset)+len(val))\n")
	buf.WriteString("\tcopy(mem[offset:], val)\n")
	buf.WriteString("\treturn true\n")
	buf.WriteString("}\n\n")

	buf.WriteString("func (m genMemory) Size() uint32 { return uint32(len(mem)) }\n")
	buf.WriteString("func (m genMemory) Definition() api.MemoryDefinition { return nil }\n")
	buf.WriteString("func (m genMemory) PageSize() uint32 { return 65536 }\n")
	buf.WriteString("func (m genMemory) Grow(delta uint32) (uint32, bool) { return uint32(len(mem) / 65536), true }\n")
	buf.WriteString("func (m genMemory) wazeroOnly() {}\n\n")
}

func (tr *Transpiler) emitExportWrappers(buf *bytes.Buffer) {
	exportMap := make(map[string]uint32)
	for _, exp := range tr.exports {
		if exp.Kind == 0 { // Func
			exportMap[exp.Name] = exp.Index
		}
	}

	buf.WriteString("var genMemInstance genMemory\n\n")

	buf.WriteString("func (b *WasmBridge) HyAbiWord() uint64 {\n")
	if f, ok := exportMap["hyrt_abi_word"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn genFn%d()\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyrtStackAlignmentProbe() uint64 {\n")
	if f, ok := exportMap["hyrt_stack_alignment_probe"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn genFn%d()\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) Alloc(size uint32) (uint32, error) {\n")
	if f, ok := exportMap["hyrt_alloc"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn uint32(genFn%d(uint64(size))), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, fmt.Errorf(\"alloc not found\")\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) Free(ptr uint32, size uint32) {\n")
	if f, ok := exportMap["hyrt_free"]; ok {
		buf.WriteString(fmt.Sprintf("\tgenFn%d(uint64(ptr), uint64(size))\n", f))
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyNop(x uint64) uint64 {\n")
	if f, ok := exportMap["hy_nop"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn genFn%d(x)\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha3Digest256(dataPtr, dataLen, outPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_sha3_digest256"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(dataPtr), uint64(dataLen), uint64(outPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha3StreamChecksum(iters, seed uint64) (uint64, error) {\n")
	if f, ok := exportMap["hy_sha3_stream_checksum"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn execWasm(%d, iters, seed), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha2256(dataPtr, dataLen, outPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_sha2_256"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(dataPtr), uint64(dataLen), uint64(outPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyFlateCompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_flate_compress"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(dataPtr), uint64(dataLen), uint64(outPtr), uint64(capPtr), uint64(outLenPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyFlateDecompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_flate_decompress"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(dataPtr), uint64(dataLen), uint64(outPtr), uint64(capPtr), uint64(outLenPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyEd25519Sign(secretPtr, msgPtr, msgLen, outSigPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_ed25519_sign"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(secretPtr), uint64(msgPtr), uint64(msgLen), uint64(outSigPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyEd25519Verify(pubkeyPtr, msgPtr, msgLen, sigPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_ed25519_verify"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(pubkeyPtr), uint64(msgPtr), uint64(msgLen), uint64(sigPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonParse(ptr, len, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_parse"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(ptr), uint64(len), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonNewObject(outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_new_object"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonNewArray(outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_new_array"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetArray(v uint64, kptr, klen, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_array"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(kptr), uint64(klen), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetInt(v uint64, kptr, klen, out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_int"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(kptr), uint64(klen), uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetString(v uint64, kptr, klen, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_string"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(kptr), uint64(klen), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayLen(arr uint64, out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_len"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, arr, uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayGet(arr uint64, i uint64, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_get"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, arr, i, uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayPushString(arr uint64, sptr, slen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_push_string"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, arr, uint64(sptr), uint64(slen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonSet(v uint64, kptr, klen uint32, value uint64) (int32, error) {\n")
	if f, ok := exportMap["hy_json_set"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(kptr), uint64(klen), value)), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonValueText(v uint64, out, capVal, outLen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_value_text"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(out), uint64(capVal), uint64(outLen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonToStringPretty(v uint64, out, capVal, outLen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_to_string_pretty"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, v, uint64(out), uint64(capVal), uint64(outLen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonRelease(h uint64) (int32, error) {\n")
	if f, ok := exportMap["hy_json_release"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, h)), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonLiveCount(out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_live_count"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(execWasm(%d, uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyRegexIsMatch(pptr, plen, tptr, tlen uint32) int64 {\n")
	if f, ok := exportMap["hy_regex_is_match"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int64(execWasm(%d, uint64(pptr), uint64(plen), uint64(tptr), uint64(tlen)))\n", f))
	} else {
		buf.WriteString("\treturn -1\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyRegexIsValid(pptr, plen uint32) int64 {\n")
	if f, ok := exportMap["hy_regex_is_valid"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int64(execWasm(%d, uint64(pptr), uint64(plen)))\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")
}
