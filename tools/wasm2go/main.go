package main

import (
	"bytes"
	"encoding/binary"
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
}

func main() {
	wasmPath := "runtime/go/hybridge/hyrt.wasm"
	if len(os.Args) > 1 {
		wasmPath = os.Args[1]
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

	outPath := "runtime/go/hybridge/hyrt_gen.go"
	if err := os.WriteFile(outPath, out.Bytes(), 0644); err != nil {
		fmt.Printf("Error writing generated Go file: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Successfully generated %s (%d bytes)\n", outPath, out.Len())
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
	buf.WriteString("\t\"github.com/tetratelabs/wazero/api\"\n")
	buf.WriteString(")\n\n")

	// Global linear memory array (16 MB initial capacity)
	buf.WriteString("var mem = make([]byte, 16777216)\n\n")

	// Shadow stack pointer initialization (Global 0)
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

	buf.WriteString(`func ensureMem(needed int) {
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

`)

	// Function Table
	tableSize := tr.tableMin
	if uint32(len(tr.elementIndices)) > tableSize {
		tableSize = uint32(len(tr.elementIndices))
	}
	if tableSize < 1 {
		tableSize = 1
	}

	buf.WriteString(fmt.Sprintf("var funcTable [%d]func(...uint64) uint64\n\n", tableSize))

	// init() block initializing Data section segments into memory and funcTable
	buf.WriteString("func init() {\n")
	for _, seg := range tr.dataSegments {
		buf.WriteString(fmt.Sprintf("\tcopy(mem[%d:], []byte{", seg.Offset))
		for _, b := range seg.Data {
			buf.WriteString(fmt.Sprintf("0x%02x, ", b))
		}
		buf.WriteString("})\n")
	}
	for i, fIdx := range tr.elementIndices {
		tIdx := tr.funcTypeIndices[fIdx-tr.numImports]
		t := tr.types[tIdx]
		argList := ""
		for p := 0; p < len(t.Params); p++ {
			if p > 0 {
				argList += ", "
			}
			argList += fmt.Sprintf("args[%d]", p)
		}
		buf.WriteString(fmt.Sprintf("\tfuncTable[%d] = func(args ...uint64) uint64 { return wasmFn_%d(%s) }\n", i, fIdx, argList))
	}
	buf.WriteString("}\n\n")

	// Emit static Go functions
	tr.emitStaticFuncs(buf)

	// Memory Wrapper
	tr.emitMemoryWrapper(buf)

	// Export Wrappers
	tr.emitExportWrappers(buf)
}

type staticFrame struct {
	kind    byte // 2=block, 3=loop, 4=if
	id      int
	startSp int
	hasElse bool
}

func (tr *Transpiler) emitStaticFuncs(buf *bytes.Buffer) {
	for fIdx, code := range tr.codeList {
		absIdx := tr.numImports + uint32(fIdx)
		tIdx := tr.funcTypeIndices[fIdx]
		t := tr.types[tIdx]
		numParams := len(t.Params)
		numRets := len(t.Results)

		// Parse locals
		pos := 0
		locCntVec := readLEB128u(code, &pos)
		numLocals := numParams
		for l := uint32(0); l < locCntVec; l++ {
			count := readLEB128u(code, &pos)
			_ = code[pos] // valType
			pos++
			numLocals += int(count)
		}
		bodyStartPos := pos

		// Pass 1: compute maxSp and usedLabels
		maxSp := 0
		sp := 0
		labelCounter := 1
		ctrlStack := []staticFrame{{kind: 2, id: 1, startSp: 0}}
		usedLabels := make(map[string]bool)

		p := bodyStartPos
		for p < len(code) {
			if sp+3 > maxSp {
				maxSp = sp + 3
			}
			op := code[p]
			p++
			switch op {
			case 0x00, 0x01: // unreachable, nop
			case 0x02, 0x03, 0x04: // block, loop, if
				_ = code[p] // blocktype
				p++
				labelCounter++
				f := staticFrame{kind: op, id: labelCounter, startSp: sp}
				if op == 0x04 { // if
					sp--
					f.startSp = sp
					usedLabels[fmt.Sprintf("label_%d_else", labelCounter)] = true
				}
				ctrlStack = append(ctrlStack, f)
			case 0x05: // else
				if len(ctrlStack) > 0 {
					idx := len(ctrlStack) - 1
					ctrlStack[idx].hasElse = true
					usedLabels[fmt.Sprintf("label_%d_end", ctrlStack[idx].id)] = true
					sp = ctrlStack[idx].startSp
				}
			case 0x0b: // end
				if len(ctrlStack) > 0 {
					f := ctrlStack[len(ctrlStack)-1]
					ctrlStack = ctrlStack[:len(ctrlStack)-1]
					if f.kind == 4 && !f.hasElse {
						usedLabels[fmt.Sprintf("label_%d_else", f.id)] = true
					}
				}
			case 0x0c: // br
				depth := int(readLEB128u(code, &p))
				if depth < len(ctrlStack) {
					target := ctrlStack[len(ctrlStack)-1-depth]
					if target.kind == 3 {
						usedLabels[fmt.Sprintf("label_%d_start", target.id)] = true
					} else {
						usedLabels[fmt.Sprintf("label_%d_end", target.id)] = true
					}
				}
			case 0x0d: // br_if
				depth := int(readLEB128u(code, &p))
				sp--
				if depth < len(ctrlStack) {
					target := ctrlStack[len(ctrlStack)-1-depth]
					if target.kind == 3 {
						usedLabels[fmt.Sprintf("label_%d_start", target.id)] = true
					} else {
						usedLabels[fmt.Sprintf("label_%d_end", target.id)] = true
					}
				}
			case 0x0e: // br_table
				cnt := int(readLEB128u(code, &p))
				sp--
				for i := 0; i <= cnt; i++ {
					tDepth := int(readLEB128u(code, &p))
					if tDepth < len(ctrlStack) {
						target := ctrlStack[len(ctrlStack)-1-tDepth]
						if target.kind == 3 {
							usedLabels[fmt.Sprintf("label_%d_start", target.id)] = true
						} else {
							usedLabels[fmt.Sprintf("label_%d_end", target.id)] = true
						}
					}
				}
			case 0x0f: // return
			case 0x10: // call
				tIndex := readLEB128u(code, &p)
				pCnt := len(tr.types[tr.funcTypeIndices[tIndex-tr.numImports]].Params)
				rCnt := len(tr.types[tr.funcTypeIndices[tIndex-tr.numImports]].Results)
				sp -= pCnt
				if rCnt > 0 {
					sp++
				}
			case 0x11: // call_indirect
				typeIdx := readLEB128u(code, &p)
				_ = readLEB128u(code, &p) // tableidx
				sp--                      // tblIdx
				pCnt := len(tr.types[typeIdx].Params)
				rCnt := len(tr.types[typeIdx].Results)
				sp -= pCnt
				if rCnt > 0 {
					sp++
				}
			case 0x1a: // drop
				sp--
			case 0x1b: // select
				sp -= 2
			case 0x20: // local.get
				_ = readLEB128u(code, &p)
				sp++
			case 0x21: // local.set
				_ = readLEB128u(code, &p)
				sp--
			case 0x22: // local.tee
				_ = readLEB128u(code, &p)
			case 0x23: // global.get
				_ = readLEB128u(code, &p)
				sp++
			case 0x24: // global.set
				_ = readLEB128u(code, &p)
				sp--
			case 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e, 0x2f,
				0x30, 0x31, 0x32, 0x33, 0x34, 0x35: // memory load
				_ = readLEB128u(code, &p) // align
				_ = readLEB128u(code, &p) // offset
			case 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e: // memory store
				_ = readLEB128u(code, &p) // align
				_ = readLEB128u(code, &p) // offset
				sp -= 2
			case 0x3f: // memory.size
				p++
				sp++
			case 0x40: // memory.grow
				p++
			case 0x41, 0x42: // const
				_ = readLEB128s(code, &p)
				sp++
			case 0x43: // f32.const
				p += 4
				sp++
			case 0x44: // f64.const
				p += 8
				sp++
			case 0x45, 0x50, 0x67, 0x68, 0x69, 0x79, 0x7a, 0x7b,
				0x8b, 0x8c, 0x8d, 0x8e, 0x8f, 0x90, 0x91, 0x99, 0x9a, 0x9b, 0x9c, 0x9d, 0x9e, 0x9f,
				0xa7, 0xa8, 0xa9, 0xaa, 0xab, 0xac, 0xad, 0xae, 0xaf, 0xb0, 0xb1, 0xb2, 0xb3, 0xb4,
				0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xbb, 0xbc, 0xbd, 0xbe, 0xbf, 0xc0, 0xc1, 0xc2, 0xc3, 0xc4: // 1 arg -> 1 res
			case 0x46, 0x47, 0x48, 0x49, 0x4a, 0x4b, 0x4c, 0x4d, 0x4e, 0x4f,
				0x51, 0x52, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a,
				0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66,
				0x6a, 0x6b, 0x6c, 0x6d, 0x6e, 0x6f, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
				0x7c, 0x7d, 0x7e, 0x7f, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a,
				0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6: // 2 args -> 1 res
				sp--
			case 0xfc:
				sub := readLEB128u(code, &p)
				if sub == 10 || sub == 11 {
					p += 2
					sp -= 3
				}
			}
			if sp+3 > maxSp {
				maxSp = sp + 3
			}
		}

		// Emit function header
		paramStr := ""
		for p := 0; p < numParams; p++ {
			if p > 0 {
				paramStr += ", "
			}
			paramStr += fmt.Sprintf("p%d uint64", p)
		}
		buf.WriteString(fmt.Sprintf("func wasmFn_%d(%s) uint64 {\n", absIdx, paramStr))

		// Pre-declare ALL variables at top of function to ensure goto jumps never cross variable declarations
		buf.WriteString("\tvar i, count, addr, srcAddr, dstAddr uint32\n")
		buf.WriteString("\t_ = i; _ = count; _ = addr; _ = srcAddr; _ = dstAddr\n")

		// Emit locals
		for l := 0; l < numParams; l++ {
			buf.WriteString(fmt.Sprintf("\tvar local%d uint64 = p%d\n", l, l))
		}
		for l := numParams; l < numLocals; l++ {
			buf.WriteString(fmt.Sprintf("\tvar local%d uint64\n", l))
		}
		for l := 0; l < numLocals; l++ {
			buf.WriteString(fmt.Sprintf("\t_ = local%d\n", l))
		}

		// Emit stack slots
		for s := 0; s < maxSp; s++ {
			buf.WriteString(fmt.Sprintf("\tvar s%d uint64\n", s))
			buf.WriteString(fmt.Sprintf("\t_ = s%d\n", s))
		}

		// Pass 2: emit statements
		sp = 0
		labelCounter = 1
		ctrlStack = []staticFrame{{kind: 2, id: 1, startSp: 0}}

		p = bodyStartPos
		for p < len(code) {
			op := code[p]
			p++
			switch op {
			case 0x00:
				buf.WriteString("\tpanic(\"wasm unreachable executed\")\n")
			case 0x01: // nop
			case 0x02: // block
				_ = code[p]
				p++
				labelCounter++
				ctrlStack = append(ctrlStack, staticFrame{kind: op, id: labelCounter, startSp: sp})
			case 0x03: // loop
				_ = code[p]
				p++
				labelCounter++
				f := staticFrame{kind: op, id: labelCounter, startSp: sp}
				ctrlStack = append(ctrlStack, f)
				if usedLabels[fmt.Sprintf("label_%d_start", f.id)] {
					buf.WriteString(fmt.Sprintf("label_%d_start:\n", f.id))
				}
			case 0x04: // if
				_ = code[p]
				p++
				labelCounter++
				sp--
				f := staticFrame{kind: op, id: labelCounter, startSp: sp}
				ctrlStack = append(ctrlStack, f)
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) == 0 { goto label_%d_else }\n", sp, f.id))
			case 0x05: // else
				if len(ctrlStack) > 0 {
					idx := len(ctrlStack) - 1
					ctrlStack[idx].hasElse = true
					buf.WriteString(fmt.Sprintf("\tgoto label_%d_end\n", ctrlStack[idx].id))
					buf.WriteString(fmt.Sprintf("label_%d_else:\n", ctrlStack[idx].id))
					sp = ctrlStack[idx].startSp
				}
			case 0x0b: // end
				if len(ctrlStack) > 0 {
					f := ctrlStack[len(ctrlStack)-1]
					ctrlStack = ctrlStack[:len(ctrlStack)-1]
					if f.kind == 4 && !f.hasElse {
						if usedLabels[fmt.Sprintf("label_%d_else", f.id)] {
							buf.WriteString(fmt.Sprintf("label_%d_else:\n", f.id))
						}
					}
					if usedLabels[fmt.Sprintf("label_%d_end", f.id)] {
						buf.WriteString(fmt.Sprintf("label_%d_end:\n", f.id))
					}
				}
			case 0x0c: // br
				depth := int(readLEB128u(code, &p))
				if depth < len(ctrlStack) {
					target := ctrlStack[len(ctrlStack)-1-depth]
					if sp > target.startSp {
						buf.WriteString(fmt.Sprintf("\ts%d = s%d\n", target.startSp, sp-1))
					}
					if target.kind == 3 {
						buf.WriteString(fmt.Sprintf("\tgoto label_%d_start\n", target.id))
					} else {
						buf.WriteString(fmt.Sprintf("\tgoto label_%d_end\n", target.id))
					}
				} else {
					if sp > 0 {
						buf.WriteString(fmt.Sprintf("\treturn s%d\n", sp-1))
					} else {
						buf.WriteString("\treturn 0\n")
					}
				}
			case 0x0d: // br_if
				depth := int(readLEB128u(code, &p))
				sp--
				if depth < len(ctrlStack) {
					target := ctrlStack[len(ctrlStack)-1-depth]
					lbl := fmt.Sprintf("label_%d_end", target.id)
					if target.kind == 3 {
						lbl = fmt.Sprintf("label_%d_start", target.id)
					}
					buf.WriteString(fmt.Sprintf("\tif s%d != 0 {\n", sp))
					if sp > target.startSp {
						buf.WriteString(fmt.Sprintf("\t\ts%d = s%d\n", target.startSp, sp-1))
					}
					buf.WriteString(fmt.Sprintf("\t\tgoto %s\n\t}\n", lbl))
				} else {
					buf.WriteString(fmt.Sprintf("\tif s%d != 0 {\n", sp))
					if sp > 0 {
						buf.WriteString(fmt.Sprintf("\t\treturn s%d\n", sp-1))
					} else {
						buf.WriteString("\t\treturn 0\n")
					}
					buf.WriteString("\t}\n")
				}
			case 0x0e: // br_table
				cnt := int(readLEB128u(code, &p))
				var targets []int
				for i := 0; i < cnt; i++ {
					targets = append(targets, int(readLEB128u(code, &p)))
				}
				defTarget := int(readLEB128u(code, &p))
				sp--
				buf.WriteString(fmt.Sprintf("\tswitch int32(s%d) {\n", sp))
				for i, tDepth := range targets {
					if tDepth < len(ctrlStack) {
						target := ctrlStack[len(ctrlStack)-1-tDepth]
						lbl := fmt.Sprintf("label_%d_end", target.id)
						if target.kind == 3 {
							lbl = fmt.Sprintf("label_%d_start", target.id)
						}
						buf.WriteString(fmt.Sprintf("\tcase %d:\n", i))
						if sp > target.startSp {
							buf.WriteString(fmt.Sprintf("\t\ts%d = s%d\n", target.startSp, sp-1))
						}
						buf.WriteString(fmt.Sprintf("\t\tgoto %s\n", lbl))
					}
				}
				if defTarget < len(ctrlStack) {
					target := ctrlStack[len(ctrlStack)-1-defTarget]
					lbl := fmt.Sprintf("label_%d_end", target.id)
					if target.kind == 3 {
						lbl = fmt.Sprintf("label_%d_start", target.id)
					}
					buf.WriteString("\tdefault:\n")
					if sp > target.startSp {
						buf.WriteString(fmt.Sprintf("\t\ts%d = s%d\n", target.startSp, sp-1))
					}
					buf.WriteString(fmt.Sprintf("\t\tgoto %s\n", lbl))
				}
				buf.WriteString("\t}\n")
			case 0x0f: // return
				if sp > 0 {
					buf.WriteString(fmt.Sprintf("\treturn s%d\n", sp-1))
				} else {
					buf.WriteString("\treturn 0\n")
				}
			case 0x10: // call
				tIndex := readLEB128u(code, &p)
				pCnt := len(tr.types[tr.funcTypeIndices[tIndex-tr.numImports]].Params)
				rCnt := len(tr.types[tr.funcTypeIndices[tIndex-tr.numImports]].Results)
				args := ""
				for a := 0; a < pCnt; a++ {
					if a > 0 {
						args += ", "
					}
					args += fmt.Sprintf("s%d", sp-pCnt+a)
				}
				sp -= pCnt
				if rCnt > 0 {
					buf.WriteString(fmt.Sprintf("\ts%d = wasmFn_%d(%s)\n", sp, tIndex, args))
					sp++
				} else {
					buf.WriteString(fmt.Sprintf("\twasmFn_%d(%s)\n", tIndex, args))
				}
			case 0x11: // call_indirect
				typeIdx := readLEB128u(code, &p)
				_ = readLEB128u(code, &p) // tableIdx
				sp--
				tblIdxVar := fmt.Sprintf("s%d", sp)
				pCnt := len(tr.types[typeIdx].Params)
				rCnt := len(tr.types[typeIdx].Results)
				args := ""
				for a := 0; a < pCnt; a++ {
					if a > 0 {
						args += ", "
					}
					args += fmt.Sprintf("s%d", sp-pCnt+a)
				}
				sp -= pCnt
				if rCnt > 0 {
					buf.WriteString(fmt.Sprintf("\ts%d = funcTable[%s](%s)\n", sp, tblIdxVar, args))
					sp++
				} else {
					buf.WriteString(fmt.Sprintf("\tfuncTable[%s](%s)\n", tblIdxVar, args))
				}
			case 0x1a: // drop
				sp--
			case 0x1b: // select
				sp -= 3
				buf.WriteString(fmt.Sprintf("\tif s%d != 0 { s%d = s%d } else { s%d = s%d }\n", sp+2, sp, sp, sp, sp+1))
				sp++
			case 0x20: // local.get
				idx := readLEB128u(code, &p)
				buf.WriteString(fmt.Sprintf("\ts%d = local%d\n", sp, idx))
				sp++
			case 0x21: // local.set
				idx := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tlocal%d = s%d\n", idx, sp))
			case 0x22: // local.tee
				idx := readLEB128u(code, &p)
				buf.WriteString(fmt.Sprintf("\tlocal%d = s%d\n", idx, sp-1))
			case 0x23: // global.get
				idx := readLEB128u(code, &p)
				buf.WriteString(fmt.Sprintf("\ts%d = wasmGlobals[%d]\n", sp, idx))
				sp++
			case 0x24: // global.set
				idx := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\twasmGlobals[%d] = s%d\n", idx, sp))
			case 0x28: // i32.load
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(binary.LittleEndian.Uint32(mem[int(uint32(s%d)+%d):]))\n", sp, sp, off))
				sp++
			case 0x29: // i64.load
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+8)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = binary.LittleEndian.Uint64(mem[int(uint32(s%d)+%d):])\n", sp, sp, off))
				sp++
			case 0x2a: // f32.load
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(binary.LittleEndian.Uint32(mem[int(uint32(s%d)+%d):]))\n", sp, sp, off))
				sp++
			case 0x2b: // f64.load
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+8)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = binary.LittleEndian.Uint64(mem[int(uint32(s%d)+%d):])\n", sp, sp, off))
				sp++
			case 0x2c: // i32.load8_s
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int8(mem[int(uint32(s%d)+%d)])))\n", sp, sp, off))
				sp++
			case 0x2d: // i32.load8_u
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(mem[int(uint32(s%d)+%d)])\n", sp, sp, off))
				sp++
			case 0x2e: // i32.load16_s
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int16(binary.LittleEndian.Uint16(mem[int(uint32(s%d)+%d):]))))\n", sp, sp, off))
				sp++
			case 0x2f: // i32.load16_u
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(binary.LittleEndian.Uint16(mem[int(uint32(s%d)+%d):]))\n", sp, sp, off))
				sp++
			case 0x30: // i64.load8_s
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int8(mem[int(uint32(s%d)+%d)])))\n", sp, sp, off))
				sp++
			case 0x31: // i64.load8_u
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(mem[int(uint32(s%d)+%d)])\n", sp, sp, off))
				sp++
			case 0x32: // i64.load16_s
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int16(binary.LittleEndian.Uint16(mem[int(uint32(s%d)+%d):]))))\n", sp, sp, off))
				sp++
			case 0x33: // i64.load16_u
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(binary.LittleEndian.Uint16(mem[int(uint32(s%d)+%d):]))\n", sp, sp, off))
				sp++
			case 0x34: // i64.load32_s
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int32(binary.LittleEndian.Uint32(mem[int(uint32(s%d)+%d):]))))\n", sp, sp, off))
				sp++
			case 0x35: // i64.load32_u
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp--
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(binary.LittleEndian.Uint32(mem[int(uint32(s%d)+%d):]))\n", sp, sp, off))
				sp++
			case 0x36: // i32.store
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint32(mem[int(uint32(s%d)+%d):], uint32(s%d))\n", sp, off, sp+1))
			case 0x37: // i64.store
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+8)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint64(mem[int(uint32(s%d)+%d):], s%d)\n", sp, off, sp+1))
			case 0x38: // f32.store
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint32(mem[int(uint32(s%d)+%d):], uint32(s%d))\n", sp, off, sp+1))
			case 0x39: // f64.store
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+8)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint64(mem[int(uint32(s%d)+%d):], s%d)\n", sp, off, sp+1))
			case 0x3a: // i32.store8
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tmem[int(uint32(s%d)+%d)] = byte(s%d)\n", sp, off, sp+1))
			case 0x3b: // i32.store16
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint16(mem[int(uint32(s%d)+%d):], uint16(s%d))\n", sp, off, sp+1))
			case 0x3c: // i64.store8
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+1)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tmem[int(uint32(s%d)+%d)] = byte(s%d)\n", sp, off, sp+1))
			case 0x3d: // i64.store16
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+2)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint16(mem[int(uint32(s%d)+%d):], uint16(s%d))\n", sp, off, sp+1))
			case 0x3e: // i64.store32
				_ = readLEB128u(code, &p)
				off := readLEB128u(code, &p)
				sp -= 2
				buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)+%d)+4)\n", sp, off))
				buf.WriteString(fmt.Sprintf("\tbinary.LittleEndian.PutUint32(mem[int(uint32(s%d)+%d):], uint32(s%d))\n", sp, off, sp+1))
			case 0x3f: // memory.size
				p++
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(len(mem) / 65536)\n", sp))
				sp++
			case 0x40: // memory.grow
				p++
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(len(mem) / 65536)\n", sp-1))
			case 0x41: // i32.const
				v := readLEB128s(code, &p)
				buf.WriteString(fmt.Sprintf("\ts%d = %d\n", sp, uint64(uint32(v))))
				sp++
			case 0x42: // i64.const
				v := readLEB128s(code, &p)
				buf.WriteString(fmt.Sprintf("\ts%d = %d\n", sp, uint64(v)))
				sp++
			case 0x43: // f32.const
				v := binary.LittleEndian.Uint32(code[p : p+4])
				p += 4
				buf.WriteString(fmt.Sprintf("\ts%d = %d\n", sp, uint64(v)))
				sp++
			case 0x44: // f64.const
				v := binary.LittleEndian.Uint64(code[p : p+8])
				p += 8
				buf.WriteString(fmt.Sprintf("\ts%d = %d\n", sp, v))
				sp++
			case 0x45: // i32.eqz
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) == 0 { s%d = 1 } else { s%d = 0 }\n", sp-1, sp-1, sp-1))
			case 0x46: // i32.eq
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) == uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x47: // i32.ne
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) != uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x48: // i32.lt_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int32(s%d) < int32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x49: // i32.lt_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) < uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4a: // i32.gt_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int32(s%d) > int32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4b: // i32.gt_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) > uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4c: // i32.le_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int32(s%d) <= int32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4d: // i32.le_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) <= uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4e: // i32.ge_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int32(s%d) >= int32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x4f: // i32.ge_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif uint32(s%d) >= uint32(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x50: // i64.eqz
				buf.WriteString(fmt.Sprintf("\tif s%d == 0 { s%d = 1 } else { s%d = 0 }\n", sp-1, sp-1, sp-1))
			case 0x51: // i64.eq
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d == s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x52: // i64.ne
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d != s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x53: // i64.lt_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int64(s%d) < int64(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x54: // i64.lt_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d < s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x55: // i64.gt_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int64(s%d) > int64(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x56: // i64.gt_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d > s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x57: // i64.ge_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int64(s%d) >= int64(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x58: // i64.ge_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d >= s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x59: // i64.le_s
				sp--
				buf.WriteString(fmt.Sprintf("\tif int64(s%d) <= int64(s%d) { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))
			case 0x5a: // i64.le_u
				sp--
				buf.WriteString(fmt.Sprintf("\tif s%d <= s%d { s%d = 1 } else { s%d = 0 }\n", sp-1, sp, sp-1, sp-1))

			case 0x67: // i32.clz
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.LeadingZeros32(uint32(s%d)))\n", sp-1, sp-1))
			case 0x68: // i32.ctz
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.TrailingZeros32(uint32(s%d)))\n", sp-1, sp-1))
			case 0x69: // i32.popcnt
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.OnesCount32(uint32(s%d)))\n", sp-1, sp-1))
			case 0x6a: // i32.add
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) + uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x6b: // i32.sub
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) - uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x6c: // i32.mul
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) * uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x6d: // i32.div_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int32(s%d) / int32(s%d)))\n", sp-1, sp-1, sp))
			case 0x6e: // i32.div_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) / uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x6f: // i32.rem_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int32(s%d) %% int32(s%d)))\n", sp-1, sp-1, sp))
			case 0x70: // i32.rem_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) %% uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x71: // i32.and
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) & uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x72: // i32.or
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) | uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x73: // i32.xor
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) ^ uint32(s%d))\n", sp-1, sp-1, sp))
			case 0x74: // i32.shl
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) << (uint32(s%d) & 31))\n", sp-1, sp-1, sp))
			case 0x75: // i32.shr_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int32(s%d) >> (uint32(s%d) & 31)))\n", sp-1, sp-1, sp))
			case 0x76: // i32.shr_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d) >> (uint32(s%d) & 31))\n", sp-1, sp-1, sp))
			case 0x77: // i32.rotl
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.RotateLeft32(uint32(s%d), int(s%d)))\n", sp-1, sp-1, sp))
			case 0x78: // i32.rotr
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.RotateLeft32(uint32(s%d), -int(s%d)))\n", sp-1, sp-1, sp))

			case 0x79: // i64.clz
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.LeadingZeros64(s%d))\n", sp-1, sp-1))
			case 0x7a: // i64.ctz
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.TrailingZeros64(s%d))\n", sp-1, sp-1))
			case 0x7b: // i64.popcnt
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(bits.OnesCount64(s%d))\n", sp-1, sp-1))
			case 0x7c: // i64.add
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d + s%d\n", sp-1, sp-1, sp))
			case 0x7d: // i64.sub
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d - s%d\n", sp-1, sp-1, sp))
			case 0x7e: // i64.mul
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d * s%d\n", sp-1, sp-1, sp))
			case 0x7f: // i64.div_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(s%d) / int64(s%d))\n", sp-1, sp-1, sp))
			case 0x80: // i64.div_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d / s%d\n", sp-1, sp-1, sp))
			case 0x81: // i64.rem_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(s%d) %% int64(s%d))\n", sp-1, sp-1, sp))
			case 0x82: // i64.rem_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d %% s%d\n", sp-1, sp-1, sp))
			case 0x83: // i64.and
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d & s%d\n", sp-1, sp-1, sp))
			case 0x84: // i64.or
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d | s%d\n", sp-1, sp-1, sp))
			case 0x85: // i64.xor
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d ^ s%d\n", sp-1, sp-1, sp))
			case 0x86: // i64.shl
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d << (s%d & 63)\n", sp-1, sp-1, sp))
			case 0x87: // i64.shr_s
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(s%d) >> (s%d & 63))\n", sp-1, sp-1, sp))
			case 0x88: // i64.shr_u
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = s%d >> (s%d & 63)\n", sp-1, sp-1, sp))
			case 0x89: // i64.rotl
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = bits.RotateLeft64(s%d, int(s%d))\n", sp-1, sp-1, sp))
			case 0x8a: // i64.rotr
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = bits.RotateLeft64(s%d, -int(s%d))\n", sp-1, sp-1, sp))

			case 0x8b: // f32.abs
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Abs(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x8c: // f32.neg
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(-math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1))
			case 0x8d: // f32.ceil
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Ceil(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x8e: // f32.floor
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Floor(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x8f: // f32.trunc
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Trunc(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x90: // f32.nearest
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.RoundToEven(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x91: // f32.sqrt
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Sqrt(float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1))
			case 0x92: // f32.add
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(math.Float32frombits(uint32(s%d)) + math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1, sp))
			case 0x93: // f32.sub
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(math.Float32frombits(uint32(s%d)) - math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1, sp))
			case 0x94: // f32.mul
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(math.Float32frombits(uint32(s%d)) * math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1, sp))
			case 0x95: // f32.div
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(math.Float32frombits(uint32(s%d)) / math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1, sp))
			case 0x96: // f32.min
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Min(float64(math.Float32frombits(uint32(s%d))), float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1, sp))
			case 0x97: // f32.max
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Max(float64(math.Float32frombits(uint32(s%d))), float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1, sp))
			case 0x98: // f32.copysign
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Copysign(float64(math.Float32frombits(uint32(s%d))), float64(math.Float32frombits(uint32(s%d)))))))\n", sp-1, sp-1, sp))

			case 0x99: // f64.abs
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Abs(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0x9a: // f64.neg
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(-math.Float64frombits(s%d))\n", sp-1, sp-1))
			case 0x9b: // f64.ceil
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Ceil(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0x9c: // f64.floor
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Floor(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0x9d: // f64.trunc
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Trunc(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0x9e: // f64.nearest
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.RoundToEven(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0x9f: // f64.sqrt
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Sqrt(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0xa0: // f64.add
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Float64frombits(s%d) + math.Float64frombits(s%d))\n", sp-1, sp-1, sp))
			case 0xa1: // f64.sub
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Float64frombits(s%d) - math.Float64frombits(s%d))\n", sp-1, sp-1, sp))
			case 0xa2: // f64.mul
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Float64frombits(s%d) * math.Float64frombits(s%d))\n", sp-1, sp-1, sp))
			case 0xa3: // f64.div
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Float64frombits(s%d) / math.Float64frombits(s%d))\n", sp-1, sp-1, sp))
			case 0xa4: // f64.min
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Min(math.Float64frombits(s%d), math.Float64frombits(s%d)))\n", sp-1, sp-1, sp))
			case 0xa5: // f64.max
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Max(math.Float64frombits(s%d), math.Float64frombits(s%d)))\n", sp-1, sp-1, sp))
			case 0xa6: // f64.copysign
				sp--
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(math.Copysign(math.Float64frombits(s%d), math.Float64frombits(s%d)))\n", sp-1, sp-1, sp))

			case 0xa7: // i32.wrap_i64
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d))\n", sp-1, sp-1))
			case 0xa8: // i32.trunc_f32_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int32(math.Float32frombits(uint32(s%d)))))\n", sp-1, sp-1))
			case 0xa9: // i32.trunc_f32_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1))
			case 0xaa: // i32.trunc_f64_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int32(math.Float64frombits(s%d))))\n", sp-1, sp-1))
			case 0xab: // i32.trunc_f64_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0xac: // i64.extend_i32_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int32(s%d)))\n", sp-1, sp-1))
			case 0xad: // i64.extend_i32_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d))\n", sp-1, sp-1))
			case 0xae: // i64.trunc_f32_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1))
			case 0xaf: // i64.trunc_f32_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32frombits(uint32(s%d)))\n", sp-1, sp-1))
			case 0xb0: // i64.trunc_f64_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(math.Float64frombits(s%d)))\n", sp-1, sp-1))
			case 0xb1: // i64.trunc_f64_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float64frombits(s%d))\n", sp-1, sp-1))
			case 0xb2: // f32.convert_i32_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(int32(s%d))))\n", sp-1, sp-1))
			case 0xb3: // f32.convert_i32_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(uint32(s%d))))\n", sp-1, sp-1))
			case 0xb4: // f32.convert_i64_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(int64(s%d))))\n", sp-1, sp-1))
			case 0xb5: // f32.convert_i64_u
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(s%d)))\n", sp-1, sp-1))
			case 0xb6: // f32.demote_f64
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(math.Float32bits(float32(math.Float64frombits(s%d))))\n", sp-1, sp-1))
			case 0xb7: // f64.convert_i32_s
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(float64(int32(s%d)))\n", sp-1, sp-1))
			case 0xb8: // f64.convert_i32_u
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(float64(uint32(s%d)))\n", sp-1, sp-1))
			case 0xb9: // f64.convert_i64_s
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(float64(int64(s%d)))\n", sp-1, sp-1))
			case 0xba: // f64.convert_i64_u
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(float64(s%d))\n", sp-1, sp-1))
			case 0xbb: // f64.promote_f32
				buf.WriteString(fmt.Sprintf("\ts%d = math.Float64bits(float64(math.Float32frombits(uint32(s%d))))\n", sp-1, sp-1))
			case 0xbc: // i32.reinterpret_f32
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d))\n", sp-1, sp-1))
			case 0xbd: // i64.reinterpret_f64
			case 0xbe: // f32.reinterpret_i32
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(s%d))\n", sp-1, sp-1))
			case 0xbf: // f64.reinterpret_i64
			case 0xc0: // i32.extend8_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int8(s%d)))\n", sp-1, sp-1))
			case 0xc1: // i32.extend16_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(uint32(int16(s%d)))\n", sp-1, sp-1))
			case 0xc2: // i64.extend8_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int8(s%d)))\n", sp-1, sp-1))
			case 0xc3: // i64.extend16_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int16(s%d)))\n", sp-1, sp-1))
			case 0xc4: // i64.extend32_s
				buf.WriteString(fmt.Sprintf("\ts%d = uint64(int64(int32(s%d)))\n", sp-1, sp-1))

			case 0xfc:
				sub := readLEB128u(code, &p)
				if sub == 10 { // memory.fill
					p += 2
					sp -= 3
					buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)) + int(uint32(s%d)))\n", sp, sp+2))
					buf.WriteString(fmt.Sprintf("\tfor i = 0; i < uint32(s%d); i++ {\n", sp+2))
					buf.WriteString(fmt.Sprintf("\t\tmem[int(uint32(s%d))+int(i)] = byte(s%d)\n\t}\n", sp, sp+1))
				} else if sub == 11 { // memory.copy
					p += 2
					sp -= 3
					buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)) + int(uint32(s%d)))\n", sp, sp+2))
					buf.WriteString(fmt.Sprintf("\tensureMem(int(uint32(s%d)) + int(uint32(s%d)))\n", sp+1, sp+2))
					buf.WriteString(fmt.Sprintf("\tcopy(mem[int(uint32(s%d)):int(uint32(s%d))+int(uint32(s%d))], mem[int(uint32(s%d)):int(uint32(s%d))+int(uint32(s%d))])\n", sp, sp, sp+2, sp+1, sp+1, sp+2))
				}
			}
		}

		for len(ctrlStack) > 0 {
			f := ctrlStack[len(ctrlStack)-1]
			ctrlStack = ctrlStack[:len(ctrlStack)-1]
			if usedLabels[fmt.Sprintf("label_%d_end", f.id)] {
				buf.WriteString(fmt.Sprintf("label_%d_end:\n", f.id))
			}
		}

		if numRets > 0 {
			if sp > 0 {
				buf.WriteString(fmt.Sprintf("\treturn s%d\n", sp-1))
			} else {
				buf.WriteString("\treturn 0\n")
			}
		} else {
			buf.WriteString("\treturn 0\n")
		}

		buf.WriteString("}\n\n")
	}
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
		buf.WriteString(fmt.Sprintf("\treturn wasmFn_%d()\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) Alloc(size uint32) (uint32, error) {\n")
	if f, ok := exportMap["hyrt_alloc"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn uint32(wasmFn_%d(uint64(size))), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, fmt.Errorf(\"alloc not found\")\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) Free(ptr uint32, size uint32) {\n")
	if f, ok := exportMap["hyrt_free"]; ok {
		buf.WriteString(fmt.Sprintf("\twasmFn_%d(uint64(ptr), uint64(size))\n", f))
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyNop(x uint64) uint64 {\n")
	if f, ok := exportMap["hy_nop"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn wasmFn_%d(x)\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha3Digest256(dataPtr, dataLen, outPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_sha3_digest256"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(dataPtr), uint64(dataLen), uint64(outPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha3StreamChecksum(iters, seed uint64) (uint64, error) {\n")
	if f, ok := exportMap["hy_sha3_stream_checksum"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn wasmFn_%d(iters, seed), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HySha2256(dataPtr, dataLen, outPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_sha2_256"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(dataPtr), uint64(dataLen), uint64(outPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyFlateCompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_flate_compress"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(dataPtr), uint64(dataLen), uint64(outPtr), uint64(capPtr), uint64(outLenPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyFlateDecompress(dataPtr, dataLen, outPtr, capPtr, outLenPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_flate_decompress"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(dataPtr), uint64(dataLen), uint64(outPtr), uint64(capPtr), uint64(outLenPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyEd25519Sign(secretPtr, msgPtr, msgLen, outSigPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_ed25519_sign"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(secretPtr), uint64(msgPtr), uint64(msgLen), uint64(outSigPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyEd25519Verify(pubkeyPtr, msgPtr, msgLen, sigPtr uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_ed25519_verify"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(pubkeyPtr), uint64(msgPtr), uint64(msgLen), uint64(sigPtr))), nil\n", f))
	} else {
		buf.WriteString("\treturn 0, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonParse(ptr, len, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_parse"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(ptr), uint64(len), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonNewObject(outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_new_object"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonNewArray(outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_new_array"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetArray(v uint64, kptr, klen, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_array"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(kptr), uint64(klen), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetInt(v uint64, kptr, klen, out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_int"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(kptr), uint64(klen), uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonGetString(v uint64, kptr, klen, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_get_string"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(kptr), uint64(klen), uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayLen(arr uint64, out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_len"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(arr, uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayGet(arr uint64, i uint64, outHandle uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_get"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(arr, i, uint64(outHandle))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonArrayPushString(arr uint64, sptr, slen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_array_push_string"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(arr, uint64(sptr), uint64(slen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonSet(v uint64, kptr, klen uint32, value uint64) (int32, error) {\n")
	if f, ok := exportMap["hy_json_set"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(kptr), uint64(klen), value)), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonValueText(v uint64, out, capVal, outLen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_value_text"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(out), uint64(capVal), uint64(outLen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonToStringPretty(v uint64, out, capVal, outLen uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_to_string_pretty"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(v, uint64(out), uint64(capVal), uint64(outLen))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonRelease(h uint64) (int32, error) {\n")
	if f, ok := exportMap["hy_json_release"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(h)), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyJsonLiveCount(out uint32) (int32, error) {\n")
	if f, ok := exportMap["hy_json_live_count"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int32(wasmFn_%d(uint64(out))), nil\n", f))
	} else {
		buf.WriteString("\treturn 1, nil\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyRegexIsMatch(pptr, plen, tptr, tlen uint32) int64 {\n")
	if f, ok := exportMap["hy_regex_is_match"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int64(wasmFn_%d(uint64(pptr), uint64(plen), uint64(tptr), uint64(tlen)))\n", f))
	} else {
		buf.WriteString("\treturn -1\n")
	}
	buf.WriteString("}\n\n")

	buf.WriteString("func (b *WasmBridge) HyRegexIsValid(pptr, plen uint32) int64 {\n")
	if f, ok := exportMap["hy_regex_is_valid"]; ok {
		buf.WriteString(fmt.Sprintf("\treturn int64(wasmFn_%d(uint64(pptr), uint64(plen)))\n", f))
	} else {
		buf.WriteString("\treturn 0\n")
	}
	buf.WriteString("}\n\n")
}
