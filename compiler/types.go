package compiler

type Type interface {
	String() string
}

type JoinHandle struct {
	T Type
}

type Chan struct {
	Dir  ChanDir
	Elem Type
}
