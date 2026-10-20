//go:build linux && amd64

// Package ipc is an SPSC, fixed-layout mmap/futex experiment, not a general ownership API.
package ipc

import (
	"encoding/binary"
	"fmt"
	"os"
	"sync/atomic"
	"syscall"
	"time"
	"unsafe"
)

const capacity = 1024
const messageSize = 64
const mappingSize = 128 + capacity*messageSize

type ring struct {
	memory     []byte
	head, tail *uint32
}

func openRing(path string) (*ring, error) {
	file, err := os.OpenFile(path, os.O_CREATE|os.O_RDWR, 0600)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	if err = file.Truncate(mappingSize); err != nil {
		return nil, err
	}
	memory, err := syscall.Mmap(int(file.Fd()), 0, mappingSize, syscall.PROT_READ|syscall.PROT_WRITE, syscall.MAP_SHARED)
	if err != nil {
		return nil, err
	}
	return &ring{memory, (*uint32)(unsafe.Pointer(&memory[0])), (*uint32)(unsafe.Pointer(&memory[64]))}, nil
}
func (r *ring) close() error { return syscall.Munmap(r.memory) }
func wait(word *uint32, value uint32) error {
	timeout := syscall.NsecToTimespec(20_000_000)
	_, _, errno := syscall.Syscall6(syscall.SYS_FUTEX, uintptr(unsafe.Pointer(word)), 0, uintptr(value), uintptr(unsafe.Pointer(&timeout)), 0, 0)
	if errno == 0 || errno == syscall.EAGAIN || errno == syscall.EINTR || errno == syscall.ETIMEDOUT {
		return nil
	}
	return errno
}
func wake(word *uint32) {
	syscall.Syscall6(syscall.SYS_FUTEX, uintptr(unsafe.Pointer(word)), 1, 1, 0, 0, 0)
}

// Linux CLOCK_MONOTONIC is system-wide, so timestamps are comparable across peers.
// A direct syscall adds measurement overhead to both transports equally.
func clockNS() int64 {
	var stamp syscall.Timespec
	_, _, errno := syscall.RawSyscall(syscall.SYS_CLOCK_GETTIME, 1, uintptr(unsafe.Pointer(&stamp)), 0)
	if errno != 0 {
		panic(errno)
	}
	return stamp.Sec*1_000_000_000 + stamp.Nsec
}
func fill(slot []byte, index uint64) {
	binary.LittleEndian.PutUint64(slot, index)
	binary.LittleEndian.PutUint64(slot[8:], uint64(clockNS()))
	for n := 2; n < 8; n++ {
		binary.LittleEndian.PutUint64(slot[n*8:], index^uint64(n))
	}
}
func verify(slot []byte, index uint64) (int64, error) {
	if got := binary.LittleEndian.Uint64(slot); got != index {
		return 0, fmt.Errorf("sequence: got %d want %d", got, index)
	}
	for n := 2; n < 8; n++ {
		if binary.LittleEndian.Uint64(slot[n*8:]) != index^uint64(n) {
			return 0, fmt.Errorf("torn payload at message %d word %d", index, n)
		}
	}
	return clockNS() - int64(binary.LittleEndian.Uint64(slot[8:])), nil
}

// Payload is written directly into a shared slot, then published by an atomic store.
func (r *ring) send(index uint64, deadline time.Time) error {
	for {
		head, tail := atomic.LoadUint32(r.head), atomic.LoadUint32(r.tail)
		if head-tail < capacity {
			slot := r.memory[128+(head%capacity)*messageSize:][:messageSize]
			fill(slot, index)
			atomic.StoreUint32(r.head, head+1)
			if head == atomic.LoadUint32(r.tail) {
				wake(r.head)
			}
			return nil
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("producer deadline")
		}
		if err := wait(r.tail, tail); err != nil {
			return err
		}
	}
}

// The slot is read before advancing tail; no view may outlive this operation.
func (r *ring) receive(index uint64, deadline time.Time) (int64, error) {
	for {
		tail, head := atomic.LoadUint32(r.tail), atomic.LoadUint32(r.head)
		if tail != head {
			slot := r.memory[128+(tail%capacity)*messageSize:][:messageSize]
			latency, err := verify(slot, index)
			if err != nil {
				return 0, err
			}
			atomic.StoreUint32(r.tail, tail+1)
			if atomic.LoadUint32(r.head)-tail >= capacity {
				wake(r.tail)
			}
			return latency, nil
		}
		if time.Now().After(deadline) {
			return 0, fmt.Errorf("consumer deadline")
		}
		if err := wait(r.head, head); err != nil {
			return 0, err
		}
	}
}
