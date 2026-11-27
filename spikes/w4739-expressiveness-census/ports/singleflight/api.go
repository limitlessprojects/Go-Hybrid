// Copyright 2013 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style
// license that can be found in the LICENSE file.

// Package singleflight provides a duplicate function call suppression
// mechanism.
package singleflight // import "golang.org/x/sync/singleflight"

import (
	"bytes"
	"errors"
	"fmt"
	"runtime"
	"runtime/debug"
	"sync"
)

// errGoexit indicates the runtime.Goexit was called in
// the user given function.
var errGoexit = errors.New("runtime.Goexit was called")

// A panicError is an arbitrary value recovered from a panic
// with the stack trace during the execution of given function.
type panicError struct {
	value interface{}
	stack []byte
}

// Error implements error interface.
func (p *panicError) Error() string {
	return fmt.Sprintf("%v\n\n%s", p.value, p.stack)
}

func (p *panicError) Unwrap() error {
	err, ok := p.value.(error)
	if !ok {
		return nil
	}

	return err
}

func newPanicError(v interface{}) error {
	stack := debug.Stack()

	// The first line of the stack trace is of the form "goroutine N [status]:"
	// but by the time the panic reaches Do the goroutine may no longer exist
	// and its status will have changed. Trim out the misleading line.
	if line := bytes.IndexByte(stack[:], '\n'); line >= 0 {
		stack = stack[line+1:]
	}
	return &panicError{value: v, stack: stack}
}

// Group forwards its public API to the Hybrid implementation.
// Panic values and type assertions remain Go compatibility escapes.
type Group = group

type Result struct {
	Val    interface{}
	Err    error
	Shared bool
}

func isPanicError(err error) bool { _, ok := err.(*panicError); return ok }
func lookupCall(store *sync.Map, key string) *call {
	value, ok := store.Load(key)
	if !ok {
		return nil
	}
	return value.(*call)
}

func (g *group) Do(key string, work func() (interface{}, error)) (v interface{}, err error, shared bool) {
	finish := func(c *call, duplicate bool) { v, err, shared = c.val, c.err, duplicate || c.dups > 0 }
	g.do(key, work, errGoexit, isPanicError, newPanicError, runtime.Goexit, lookupCall, finish, putCall, dropCall)
	return
}

func (g *group) DoChan(key string, work func() (interface{}, error)) <-chan Result {
	ch := make(chan Result, 1)
	deliver := func(v interface{}, err error, shared bool) { ch <- Result{v, err, shared} }
	g.do_chan(key, work, deliver, errGoexit, isPanicError, newPanicError, runtime.Goexit, lookupCall, func(*call, bool) {}, putCall, dropCall)
	return ch
}

func (g *group) Forget(key string) { g.forget(key, dropCall) }

func putCall(store *sync.Map, key string, c *call) { store.Store(key, c) }
func dropCall(store *sync.Map, key string)         { store.Delete(key) }
