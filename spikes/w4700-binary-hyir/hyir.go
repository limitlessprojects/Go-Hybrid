// Package hyir implements the versioned binary intermediate representation (hyir v1)
// and go/ast materializer for Go-Hybrid.
//
// Answering Proposal #4700: "[wildcard] Delete the Go-text emitter: checked graph lowers to a binary IR materialized as go/ast"
package hyir

import (
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"go/ast"
	"go/token"
)

// Magic bytes: 0x7F, 'H', 'Y', 'I', 'R'
var MagicHeader = [5]byte{0x7F, 'H', 'Y', 'I', 'R'}
const SchemaVersion uint8 = 1

// NodeKind identifies the serialized AST/IR element.
type NodeKind uint8

const (
	KindPackage NodeKind = 1
	KindFuncDecl NodeKind = 2
	KindTaskSpawn NodeKind = 3
	KindChanMake NodeKind = 4
	KindChanSend NodeKind = 5
	KindChanRecv NodeKind = 6
	KindCall NodeKind = 7
	KindReturn NodeKind = 8
	KindLiteralString NodeKind = 9
	KindLiteralInt NodeKind = 10
)

// EffectCertificate represents compile-time checked proof flags attached as binary metadata.
type EffectCertificate uint16

const (
	EffectNone      EffectCertificate = 0
	EffectAffine    EffectCertificate = 1 << 0
	EffectSendable  EffectCertificate = 1 << 1
	EffectLinear    EffectCertificate = 1 << 2
	EffectExclusive EffectCertificate = 1 << 3
)

// SourceSpan preserves byte offsets for zero-overhead diagnostic mapping.
type SourceSpan struct {
	StartOffset uint32
	EndOffset   uint32
}

// IRNode represents an abstract node in the typed hyir graph.
type IRNode struct {
	Kind        NodeKind
	Span        SourceSpan
	Certificate EffectCertificate
	Name        string
	TypeStr     string
	ValueStr    string
	ValueInt    int64
	Children    []*IRNode
}

// Encoder serializes typed IRNodes into a compact binary byte stream.
type Encoder struct {
	buf bytes.Buffer
}

func NewEncoder() *Encoder {
	enc := &Encoder{}
	enc.buf.Write(MagicHeader[:])
	enc.buf.WriteByte(SchemaVersion)
	return enc
}

func (e *Encoder) EncodeNode(node *IRNode) error {
	if node == nil {
		e.buf.WriteByte(0)
		return nil
	}
	e.buf.WriteByte(byte(node.Kind))

	// Span (8 bytes)
	if err := binary.Write(&e.buf, binary.BigEndian, node.Span.StartOffset); err != nil {
		return err
	}
	if err := binary.Write(&e.buf, binary.BigEndian, node.Span.EndOffset); err != nil {
		return err
	}

	// Effect certificate (2 bytes)
	if err := binary.Write(&e.buf, binary.BigEndian, uint16(node.Certificate)); err != nil {
		return err
	}

	// Name string
	writeString(&e.buf, node.Name)
	// Type string
	writeString(&e.buf, node.TypeStr)
	// Value string
	writeString(&e.buf, node.ValueStr)

	// ValueInt (8 bytes)
	if err := binary.Write(&e.buf, binary.BigEndian, node.ValueInt); err != nil {
		return err
	}

	// Children count (2 bytes)
	numChildren := uint16(len(node.Children))
	if err := binary.Write(&e.buf, binary.BigEndian, numChildren); err != nil {
		return err
	}

	for _, child := range node.Children {
		if err := e.EncodeNode(child); err != nil {
			return err
		}
	}
	return nil
}

func (e *Encoder) Bytes() []byte {
	return e.buf.Bytes()
}

// Decoder parses a binary hyir byte stream back into typed IRNodes.
type Decoder struct {
	reader *bytes.Reader
}

func NewDecoder(data []byte) (*Decoder, error) {
	if len(data) < 6 {
		return nil, errors.New("HYB-IR-E04: truncated binary stream, missing header")
	}
	for i := 0; i < 5; i++ {
		if data[i] != MagicHeader[i] {
			return nil, fmt.Errorf("HYB-IR-E02: invalid magic header: %v", data[:5])
		}
	}
	version := data[5]
	if version != SchemaVersion {
		return nil, fmt.Errorf("HYB-IR-E03: unsupported schema version: %d (expected %d)", version, SchemaVersion)
	}
	return &Decoder{reader: bytes.NewReader(data[6:])}, nil
}

func (d *Decoder) DecodeNode() (*IRNode, error) {
	kindByte, err := d.reader.ReadByte()
	if err != nil {
		return nil, err
	}
	if kindByte == 0 {
		return nil, nil
	}

	node := &IRNode{Kind: NodeKind(kindByte)}

	if err := binary.Read(d.reader, binary.BigEndian, &node.Span.StartOffset); err != nil {
		return nil, err
	}
	if err := binary.Read(d.reader, binary.BigEndian, &node.Span.EndOffset); err != nil {
		return nil, err
	}

	var cert uint16
	if err := binary.Read(d.reader, binary.BigEndian, &cert); err != nil {
		return nil, err
	}
	node.Certificate = EffectCertificate(cert)

	node.Name, err = readString(d.reader)
	if err != nil {
		return nil, err
	}
	node.TypeStr, err = readString(d.reader)
	if err != nil {
		return nil, err
	}
	node.ValueStr, err = readString(d.reader)
	if err != nil {
		return nil, err
	}

	if err := binary.Read(d.reader, binary.BigEndian, &node.ValueInt); err != nil {
		return nil, err
	}

	var numChildren uint16
	if err := binary.Read(d.reader, binary.BigEndian, &numChildren); err != nil {
		return nil, err
	}

	node.Children = make([]*IRNode, numChildren)
	for i := 0; i < int(numChildren); i++ {
		child, err := d.DecodeNode()
		if err != nil {
			return nil, err
		}
		node.Children[i] = child
	}
	return node, nil
}

// MaterializeAst translates a typed IRNode directly into go/ast structures without text emission.
func MaterializeAst(root *IRNode, fset *token.FileSet) (*ast.File, error) {
	if root == nil || root.Kind != KindPackage {
		return nil, errors.New("root node must be a KindPackage")
	}

	file := &ast.File{
		Name:  ast.NewIdent(root.Name),
		Decls: []ast.Decl{},
	}

	for _, child := range root.Children {
		if child.Kind == KindFuncDecl {
			fnDecl, err := materializeFuncDecl(child)
			if err != nil {
				return nil, err
			}
			file.Decls = append(file.Decls, fnDecl)
		}
	}
	return file, nil
}

func materializeFuncDecl(node *IRNode) (*ast.FuncDecl, error) {
	fn := &ast.FuncDecl{
		Name: ast.NewIdent(node.Name),
		Type: &ast.FuncType{
			Params:  &ast.FieldList{List: []*ast.Field{}},
			Results: &ast.FieldList{List: []*ast.Field{}},
		},
		Body: &ast.BlockStmt{List: []ast.Stmt{}},
	}

	for _, stmtNode := range node.Children {
		switch stmtNode.Kind {
		case KindTaskSpawn:
			// Lowers directly to ast.GoStmt (Ordinary Go goroutine scheduler!)
			goStmt := &ast.GoStmt{
				Call: &ast.CallExpr{
					Fun:  ast.NewIdent(stmtNode.Name),
					Args: []ast.Expr{},
				},
			}
			fn.Body.List = append(fn.Body.List, goStmt)
		case KindCall:
			callStmt := &ast.ExprStmt{
				X: &ast.CallExpr{
					Fun:  ast.NewIdent(stmtNode.Name),
					Args: []ast.Expr{},
				},
			}
			fn.Body.List = append(fn.Body.List, callStmt)
		case KindReturn:
			retStmt := &ast.ReturnStmt{
				Results: []ast.Expr{ast.NewIdent(stmtNode.Name)},
			}
			fn.Body.List = append(fn.Body.List, retStmt)
		}
	}
	return fn, nil
}

func writeString(buf *bytes.Buffer, s string) {
	b := []byte(s)
	binary.Write(buf, binary.BigEndian, uint16(len(b)))
	buf.Write(b)
}

func readString(r *bytes.Reader) (string, error) {
	var l uint16
	if err := binary.Read(r, binary.BigEndian, &l); err != nil {
		return "", err
	}
	b := make([]byte, l)
	if _, err := r.Read(b); err != nil {
		return "", err
	}
	return string(b), nil
}
