package geometry

import (
	"fmt"
	"os"
	"strings"
	"testing"
)

// This file implements the pinned A4 RSS-capture protocol for the Phase
// 0 performance rerun (GNU time is absent on the reference host):
//
//   - The runner sets DOMOWIK_RSS_CASE to exactly one corpus case name
//     and invokes this test binary once for that case (one process per
//     selector; no child processes, dead-PID reads, polling, or
//     allocation-based substitution).
//   - After testing completes and before THIS process exits, TestMain
//     reads /proc/self/status in-process and prints its own VmHWM line.
//     The kernel reports VmHWM in kB with 1024-byte units, i.e. KiB;
//     tracker 0030 parses it via phase0.ParseVmHWMKiB and judges it
//     against phase0.CheckRSSLimit (< 524288 KiB).
//
// When DOMOWIK_RSS_CASE is unset (all ordinary test and benchmark runs)
// TestMain adds nothing to the output.

func TestMain(m *testing.M) {
	code := m.Run()
	if os.Getenv("DOMOWIK_RSS_CASE") != "" {
		data, err := os.ReadFile("/proc/self/status")
		if err != nil {
			fmt.Fprintf(os.Stderr, "phase0 RSS capture: read /proc/self/status: %v\n", err)
			os.Exit(3)
		}
		printed := 0
		for _, line := range strings.Split(string(data), "\n") {
			if strings.HasPrefix(line, "VmHWM:") {
				fmt.Println(strings.TrimSpace(line))
				printed++
				break
			}
		}
		if printed == 0 {
			fmt.Fprintln(os.Stderr, "phase0 RSS capture: no VmHWM line found")
			os.Exit(3)
		}
	}
	os.Exit(code)
}
