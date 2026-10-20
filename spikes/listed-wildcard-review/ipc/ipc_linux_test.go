//go:build linux && amd64

package ipc

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestRingPeer(t *testing.T) {
	mode := os.Getenv("HYBRID_IPC_PEER")
	if mode == "" {
		t.Skip("subprocess entry")
	}
	path := os.Getenv("HYBRID_IPC_PATH")
	count, err := strconv.Atoi(os.Getenv("HYBRID_IPC_COUNT"))
	if err != nil {
		t.Fatal(err)
	}
	latency := make([]int64, count)
	deadline := time.Now().Add(60 * time.Second)
	if mode == "ring" {
		ring, err := openRing(path)
		if err != nil {
			t.Fatal(err)
		}
		defer ring.close()
		for n := 0; n < count; n++ {
			latency[n], err = ring.receive(uint64(n), deadline)
			if err != nil {
				t.Fatal(err)
			}
		}
	} else {
		listener, err := net.Listen("unix", path)
		if err != nil {
			t.Fatal(err)
		}
		defer listener.Close()
		connection, err := listener.Accept()
		if err != nil {
			t.Fatal(err)
		}
		defer connection.Close()
		connection.SetDeadline(deadline)
		slot := make([]byte, messageSize)
		for n := 0; n < count; n++ {
			if _, err = io.ReadFull(connection, slot); err != nil {
				t.Fatal(err)
			}
			latency[n], err = verify(slot, uint64(n))
			if err != nil {
				t.Fatal(err)
			}
		}
	}
	sort.Slice(latency, func(i, j int) bool { return latency[i] < latency[j] })
	if latency[0] < 0 {
		t.Fatal("system monotonic clock moved backwards; latency evidence invalid")
	}
	result, _ := json.Marshal(map[string]any{"mode": mode, "count": count, "p50_ns": latency[count/2], "p99_ns": latency[count*99/100]})
	fmt.Println("IPC_RESULT", string(result))
}
func TestSharedRingAndUnixMillionMessages(t *testing.T) {
	for _, mode := range []string{"ring", "unix"} {
		t.Run(mode, func(t *testing.T) {
			directory, err := os.MkdirTemp("/dev/shm", "hybrid-ipc-")
			if err != nil {
				t.Fatal(err)
			}
			defer os.RemoveAll(directory)
			path := filepath.Join(directory, "transport")
			const count = 1_000_000
			var shared *ring
			if mode == "ring" {
				shared, err = openRing(path)
				if err != nil {
					t.Fatal(err)
				}
				defer shared.close()
			}
			child := exec.Command(os.Args[0], "-test.run=^TestRingPeer$", "-test.v")
			child.Env = append(os.Environ(), "HYBRID_IPC_PEER="+mode, "HYBRID_IPC_PATH="+path, "HYBRID_IPC_COUNT="+strconv.Itoa(count))
			var output bytes.Buffer
			child.Stdout = &output
			child.Stderr = &output
			if err = child.Start(); err != nil {
				t.Fatal(err)
			}
			defer func() {
				if child.ProcessState == nil {
					child.Process.Kill()
					child.Wait()
				}
			}()
			deadline := time.Now().Add(60 * time.Second)
			start := time.Now()
			if mode == "ring" {
				for n := 0; n < count; n++ {
					if err = shared.send(uint64(n), deadline); err != nil {
						t.Fatalf("%v\n%s", err, output.String())
					}
				}
			} else {
				var connection net.Conn
				for time.Now().Before(deadline) {
					connection, err = net.Dial("unix", path)
					if err == nil {
						break
					}
					time.Sleep(time.Millisecond)
				}
				if err != nil {
					t.Fatal(err)
				}
				connection.SetDeadline(deadline)
				defer connection.Close()
				slot := make([]byte, messageSize)
				for n := 0; n < count; n++ {
					fill(slot, uint64(n))
					for rest := slot; len(rest) > 0; {
						sent, err := connection.Write(rest)
						if err != nil {
							t.Fatal(err)
						}
						rest = rest[sent:]
					}
				}
			}
			if err = child.Wait(); err != nil {
				t.Fatalf("peer: %v\n%s", err, output.String())
			}
			elapsed := time.Since(start)
			if !strings.Contains(output.String(), "IPC_RESULT") {
				t.Fatal("missing peer result")
			}
			t.Logf("%s: %d intact 64-byte messages in %s = %.0f msg/sec\n%s", mode, count, elapsed, float64(count)/elapsed.Seconds(), output.String())
		})
	}
}
func TestSequenceWrapAndPeerTimeout(t *testing.T) {
	dir := t.TempDir()
	r, err := openRing(filepath.Join(dir, "ring"))
	if err != nil {
		t.Fatal(err)
	}
	defer r.close()
	atomic.StoreUint32(r.head, ^uint32(0))
	atomic.StoreUint32(r.tail, ^uint32(0))
	if err = r.send(7, time.Now().Add(time.Second)); err != nil {
		t.Fatal(err)
	}
	if _, err = r.receive(7, time.Now().Add(time.Second)); err != nil {
		t.Fatal(err)
	}
	if _, err = r.receive(8, time.Now().Add(25*time.Millisecond)); err == nil {
		t.Fatal("missing peer did not time out")
	}
}
