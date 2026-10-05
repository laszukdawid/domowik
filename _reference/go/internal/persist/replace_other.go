//go:build !windows

package persist

import "os"

// Platform dispatch for the §9.7 S4 atomic-replace primitive on POSIX
// profiles: rename(2) within the same directory (P1/P5). The Linux/ext4
// power-loss durability claim additionally requires file fsync (S3) and
// parent-directory fsync (S5), both performed by the save machine, and
// remains gated on the fault-injection suite passing per §9.7.
func platformReplace(tmpPath, targetPath string) error {
	return os.Rename(tmpPath, targetPath)
}
