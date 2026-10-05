package sizecorpus

import (
	"fmt"
	"os"
	"runtime"
	"strconv"
	"strings"
	"syscall"
)

// Environment records the actual measurement environment for the evidence
// records (AC field "OS/filesystem"; ED-0024 requires the real local
// environment, never a fabricated reference platform).
type Environment struct {
	OS          string `json:"os"`
	Kernel      string `json:"kernel"`
	CPUModel    string `json:"cpuModel"`
	LogicalCPUs int    `json:"logicalCpus"`
	MemTotalKiB int64  `json:"memTotalKiB"`
	Filesystem  string `json:"filesystem"`
	GoVersion   string `json:"goVersion"`
	GOOS        string `json:"goos"`
	GOARCH      string `json:"goarch"`
	GOTOOLCHAIN string `json:"gotoolchain"`
}

// Capture reads the host facts of the current process. Linux sysfs/procfs
// values degrade to "unavailable" off-Linux; measurements are only claimed
// on environments where they are available.
func Capture() Environment {
	env := Environment{
		OS:          runtime.GOOS,
		Kernel:      readFirstLine("/proc/sys/kernel/osrelease", "unavailable"),
		CPUModel:    cpuModel(),
		LogicalCPUs: runtime.NumCPU(),
		MemTotalKiB: memTotalKiB(),
		Filesystem:  filesystemType("."),
		GoVersion:   runtime.Version(),
		GOOS:        runtime.GOOS,
		GOARCH:      runtime.GOARCH,
		GOTOOLCHAIN: os.Getenv("GOTOOLCHAIN"),
	}
	if env.Kernel != "unavailable" {
		env.OS = "linux"
	}
	return env
}

func readFirstLine(path, fallback string) string {
	b, err := os.ReadFile(path)
	if err != nil {
		return fallback
	}
	return strings.TrimSpace(string(b))
}

func cpuModel() string {
	b, err := os.ReadFile("/proc/cpuinfo")
	if err != nil {
		return "unavailable"
	}
	for _, line := range strings.Split(string(b), "\n") {
		if strings.HasPrefix(line, "model name") {
			if i := strings.IndexByte(line, ':'); i >= 0 {
				return strings.TrimSpace(line[i+1:])
			}
		}
	}
	return "unavailable"
}

func memTotalKiB() int64 {
	b, err := os.ReadFile("/proc/meminfo")
	if err != nil {
		return -1
	}
	for _, line := range strings.Split(string(b), "\n") {
		if strings.HasPrefix(line, "MemTotal:") {
			fields := strings.Fields(line)
			if len(fields) >= 2 {
				if v, err := strconv.ParseInt(fields[1], 10, 64); err == nil {
					return v
				}
			}
		}
	}
	return -1
}

// filesystemType maps statfs magic numbers of common local filesystems;
// unknown magics render as hex so nothing is ever invented.
func filesystemType(path string) string {
	var st syscall.Statfs_t
	if err := syscall.Statfs(path, &st); err != nil {
		return "unavailable"
	}
	switch st.Type {
	case 0x9123683E:
		return "btrfs"
	case 0xEF53:
		return "ext4"
	case 0x58465342:
		return "xfs"
	case 0x01021994:
		return "tmpfs"
	default:
		return fmt.Sprintf("0x%x", st.Type)
	}
}
