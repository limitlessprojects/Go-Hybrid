// Package ir holds the reversible-region IR for the #4596 wildcard spike.
//
// State model: 8 owned 64-bit registers x0..x7 plus one immutable word
// (`key`) at index KeyReg. Everything is fixed width: arithmetic is mod 2^64,
// the byte-wise S-box acts mod 2^8 per byte, little endian.
package ir

import (
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

const (
	// KeyReg is the index of the immutable key word inside State.
	KeyReg = 8
	// NumRegs is the number of owned registers.
	NumRegs = 8
)

// State is the region value state: x0..x7 then the immutable key.
type State [9]uint64

// Op identifies one IR statement.
type Op int

const (
	OpAdd Op = iota // x[r] += imm
	OpSub           // x[r] -= imm
	OpAddV          // x[r] += x[s]
	OpSubV          // x[r] -= x[s]
	OpXor           // x[r] ^= x[s]
	OpXorI          // x[r] ^= imm
	OpRotl          // x[r] = rotl(x[r], imm)
	OpRotr          // x[r] = rotr(x[r], imm)
	OpSwap          // x[a], x[b] = x[b], x[a]
	OpPerm          // vector permutation new[i] = old[p[i]]
	OpSbox          // byte-wise bijective table substitution
	OpIf            // conditional, discriminator must be preserved
	// below: refused statements, parsed so the checker can name them
	OpSet
	OpMask
	OpShl
	OpShr
	OpMul
	OpLookup
	OpLoop
	OpCall
	OpChanSend
	OpAlias
	OpCapture
)

var opNames = map[Op]string{
	OpAdd: "add", OpSub: "sub", OpAddV: "addv", OpSubV: "subv",
	OpXor: "xor", OpXorI: "xori", OpRotl: "rotl", OpRotr: "rotr",
	OpSwap: "swap", OpPerm: "perm", OpSbox: "sboxb", OpIf: "if",
	OpSet: "set", OpMask: "mask", OpShl: "shl", OpShr: "shr",
	OpMul: "mul", OpLookup: "lookup", OpLoop: "loop", OpCall: "call",
	OpChanSend: "chansend", OpAlias: "alias", OpCapture: "capture",
}

func (o Op) String() string {
	if n, ok := opNames[o]; ok {
		return n
	}
	return fmt.Sprintf("op(%d)", int(o))
}

// Sbox is a declared byte substitution table with its inverse contract.
type Sbox struct {
	Name      string
	A, B      byte
	Table     [256]byte
	Inv       [256]byte
	Bijective bool
}

// NewAffineSbox builds the declared table y = ((x*A) mod 256) XOR B and checks
// that it is a bijection (true iff A is odd). The inverse table is derived by
// table inversion, never by trusting the caller.
func NewAffineSbox(name string, a, b byte) *Sbox {
	sb := &Sbox{Name: name, A: a, B: b}
	var seen [256]bool
	bijective := true
	for i := 0; i < 256; i++ {
		v := byte(uint16(i)*uint16(a)) ^ b
		sb.Table[i] = v
		if seen[v] {
			bijective = false
		}
		seen[v] = true
	}
	sb.Bijective = bijective
	if bijective {
		for i := 0; i < 256; i++ {
			sb.Inv[sb.Table[i]] = byte(i)
		}
	}
	return sb
}

// Stmt is one IR statement. Field use depends on Op.
type Stmt struct {
	Op      Op
	Line    int
	Reg     int    // destination / operand 0 (or discriminator register for OpIf)
	Reg2    int    // second operand
	Imm     uint64 // immediate
	Perm    [8]int
	Sbox    string
	InvSbox bool // emit/interpret the inverse table (set by Invert)
	Bit     int  // discriminator bit index
	BitVal  uint64
	Foreign bool // discriminator comes from a non-owned value
	ID      int  // stable if-id, shared between forward and inverse program
	Then    []*Stmt
	Else    []*Stmt
}

// Program is a parsed case.
type Program struct {
	Name    string
	Stmts   []*Stmt
	Sboxes  map[string]*Sbox
	Meta    map[string]string // from `# key: value` header comments
	IfCount int
}

// MetaOr returns a header value or a default.
func (p *Program) MetaOr(k, def string) string {
	if v, ok := p.Meta[k]; ok {
		return v
	}
	return def
}

// ParseFile reads and parses one case file.
func ParseFile(path string) (*Program, error) {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	base := strings.TrimSuffix(filepath.Base(path), filepath.Ext(path))
	p, err := Parse(base, string(b))
	if err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return p, nil
}

func parseReg(tok string) (int, error) {
	if tok == "key" {
		return KeyReg, nil
	}
	if len(tok) == 2 && (tok[0] == 'x') && tok[1] >= '0' && tok[1] <= '7' {
		return int(tok[1] - '0'), nil
	}
	return 0, fmt.Errorf("bad register %q", tok)
}

func parseU64(tok string) (uint64, error) {
	s := strings.TrimSuffix(strings.TrimSuffix(tok, "u64"), "ULL")
	v, err := strconv.ParseUint(s, 0, 64)
	if err != nil {
		return 0, fmt.Errorf("bad immediate %q", tok)
	}
	return v, nil
}

func parseU8(tok string) (byte, error) {
	v, err := strconv.ParseUint(tok, 0, 8)
	if err != nil {
		return 0, fmt.Errorf("bad byte %q", tok)
	}
	return byte(v), nil
}

// Parse parses one case from text.
//
// Syntax (one statement per line, `#` starts a comment):
//
//	program NAME
//	sbox NAME affine A B
//	add x0, 0x9e3779b97f4a7c15      sub x1, 3
//	addv x0, x1                     subv x0, x2
//	xor x0, x3                      xori x0, 0xff      (or: xori x0, key)
//	rotl x0, 7                      rotr x1, 13
//	swap x0, x1                     perm 3 0 2 1 6 7 4 5
//	sboxb x0, S1
//	if bit x7, 0 == 0               if bit key, 4 == 1      if foreign bit 3 == 1
//	else
//	end
//	set x0, 0        mask x0, 0x7f   shl x0, 3   shr x0, 3   mul x0, 5   lookup x0
//	chansend x0      call "peer"     alias "buf" capture "task" loop 8
func Parse(name, text string) (*Program, error) {
	p := &Program{Name: name, Sboxes: map[string]*Sbox{}, Meta: map[string]string{}}
	type iframe struct {
		parent *[]*Stmt
		st     *Stmt
	}
	cur := &p.Stmts
	var stack []iframe

	lines := strings.Split(strings.ReplaceAll(text, "\r\n", "\n"), "\n")
	for i, raw := range lines {
		ln := i + 1
		line := raw
		if idx := strings.IndexByte(line, '#'); idx >= 0 {
			comment := strings.TrimSpace(line[idx+1:])
			line = line[:idx]
			if k, v, ok := strings.Cut(comment, ":"); ok {
				key := strings.ToLower(strings.TrimSpace(k))
				if _, dup := p.Meta[key]; !dup {
					p.Meta[key] = strings.TrimSpace(v)
				}
			}
		}
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		f := strings.Fields(strings.ReplaceAll(line, ",", " "))
		if len(f) == 0 {
			continue
		}
		add := func(st *Stmt) { *cur = append(*cur, st) }
		switch f[0] {
		case "program":
			if len(f) != 2 {
				return nil, fmt.Errorf("line %d: usage: program NAME", ln)
			}
			p.Name = f[1]
		case "sbox":
			if len(f) != 5 || f[2] != "affine" {
				return nil, fmt.Errorf("line %d: usage: sbox NAME affine A B", ln)
			}
			a, err := parseU8(f[3])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			b, err := parseU8(f[4])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			p.Sboxes[f[1]] = NewAffineSbox(f[1], a, b)
		case "if":
			st := &Stmt{Op: OpIf, Line: ln, Reg: 0}
			idx := 1
			if f[idx] == "foreign" {
				st.Foreign = true
				idx++
			}
			if f[idx] != "bit" {
				return nil, fmt.Errorf("line %d: usage: if bit REG, BIT == 0|1", ln)
			}
			idx++
			if st.Foreign {
				bit, err := strconv.Atoi(f[idx])
				if err != nil {
					return nil, fmt.Errorf("line %d: bad bit %q", ln, f[idx])
				}
				st.Reg = -1
				st.Bit = bit
			} else {
				r, err := parseReg(f[idx])
				if err != nil {
					return nil, fmt.Errorf("line %d: %w", ln, err)
				}
				st.Reg = r
				idx++
				bit, err := strconv.Atoi(f[idx])
				if err != nil {
					return nil, fmt.Errorf("line %d: bad bit %q", ln, f[idx])
				}
				st.Bit = bit
			}
			idx++
			if f[idx] != "==" {
				return nil, fmt.Errorf("line %d: expected ==", ln)
			}
			v, err := parseU64(f[idx+1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			st.BitVal = v
			st.ID = p.IfCount
			p.IfCount++
			add(st)
			stack = append(stack, iframe{parent: cur, st: st})
			cur = &st.Then
		case "else":
			if len(stack) == 0 {
				return nil, fmt.Errorf("line %d: else without if", ln)
			}
			cur = &stack[len(stack)-1].st.Else
		case "end":
			if len(stack) == 0 {
				return nil, fmt.Errorf("line %d: end without if", ln)
			}
			top := stack[len(stack)-1]
			stack = stack[:len(stack)-1]
			cur = top.parent
		case "add", "sub", "xori":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: %s REG, IMM", ln, f[0])
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			if f[2] == "key" {
				op := map[string]Op{"add": OpAddV, "sub": OpSubV, "xori": OpXor}[f[0]]
				add(&Stmt{Op: op, Line: ln, Reg: r, Reg2: KeyReg})
				continue
			}
			imm, err := parseU64(f[2])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: map[string]Op{"add": OpAdd, "sub": OpSub, "xori": OpXorI}[f[0]], Line: ln, Reg: r, Imm: imm})
		case "addv", "subv", "xor", "swap":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: %s REG, REG", ln, f[0])
			}
			r1, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			r2, err := parseReg(f[2])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			// `add x0, key` normalises to OpAddV, so do not reject r2==KeyReg here.
			op := map[string]Op{"addv": OpAddV, "subv": OpSubV, "xor": OpXor, "swap": OpSwap}[f[0]]
			add(&Stmt{Op: op, Line: ln, Reg: r1, Reg2: r2})
		case "rotl", "rotr":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: %s REG, N", ln, f[0])
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			imm, err := parseU64(f[2])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: map[string]Op{"rotl": OpRotl, "rotr": OpRotr}[f[0]], Line: ln, Reg: r, Imm: imm})
		case "perm":
			if len(f) != 9 {
				return nil, fmt.Errorf("line %d: usage: perm p0 .. p7", ln)
			}
			var st = &Stmt{Op: OpPerm, Line: ln}
			for k := 0; k < 8; k++ {
				v, err := strconv.Atoi(f[k+1])
				if err != nil || v < 0 || v > 7 {
					return nil, fmt.Errorf("line %d: bad perm index %q", ln, f[k+1])
				}
				st.Perm[k] = v
			}
			add(st)
		case "sboxb":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: sboxb REG, NAME", ln)
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: OpSbox, Line: ln, Reg: r, Sbox: f[2]})
		case "set", "mask", "shl", "shr":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: %s REG, IMM", ln, f[0])
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			imm, err := parseU64(f[2])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			op := map[string]Op{"set": OpSet, "mask": OpMask, "shl": OpShl, "shr": OpShr}[f[0]]
			add(&Stmt{Op: op, Line: ln, Reg: r, Imm: imm})
		case "mul":
			if len(f) != 3 {
				return nil, fmt.Errorf("line %d: usage: mul REG, IMM", ln)
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			imm, err := parseU64(f[2])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: OpMul, Line: ln, Reg: r, Imm: imm})
		case "lookup", "chansend":
			if len(f) != 2 {
				return nil, fmt.Errorf("line %d: usage: %s REG", ln, f[0])
			}
			r, err := parseReg(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: map[string]Op{"lookup": OpLookup, "chansend": OpChanSend}[f[0]], Line: ln, Reg: r})
		case "call", "alias", "capture":
			if len(f) != 2 {
				return nil, fmt.Errorf("line %d: usage: %s NAME", ln, f[0])
			}
			add(&Stmt{Op: map[string]Op{"call": OpCall, "alias": OpAlias, "capture": OpCapture}[f[0]], Line: ln, Sbox: strings.Trim(f[1], `"`)})
		case "loop":
			if len(f) != 2 {
				return nil, fmt.Errorf("line %d: usage: loop N", ln)
			}
			imm, err := parseU64(f[1])
			if err != nil {
				return nil, fmt.Errorf("line %d: %w", ln, err)
			}
			add(&Stmt{Op: OpLoop, Line: ln, Imm: imm})
		default:
			return nil, fmt.Errorf("line %d: unknown statement %q", ln, f[0])
		}
	}
	if len(stack) != 0 {
		return nil, fmt.Errorf("unterminated if (%d open)", len(stack))
	}
	return p, nil
}

// ByteWiseSbox applies a 256-byte table to each of the 8 bytes of v.
func ByteWiseSbox(v uint64, t *[256]byte) uint64 {
	var out uint64
	for i := 0; i < 8; i++ {
		b := byte(v >> (8 * uint(i)))
		out |= uint64(t[b]) << (8 * uint(i))
	}
	return out
}
