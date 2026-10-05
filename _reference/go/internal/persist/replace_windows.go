//go:build windows

package persist

import (
	"os"
	"syscall"
	"unsafe"
)

// Platform dispatch for the §9.7 S4 atomic-replace primitive. The NTFS
// profile uses ReplaceFileW per FORMAT-SPEC §9.7 (P3); sharing-violation
// specifics remain deferred empirics (X6). The power-loss durability
// CLAIM for this profile still requires its fault-injection suite to
// pass on validated platforms; shipping the primitive establishes the
// mechanism only, never the claim.

var (
	modkernel32      = syscall.NewLazyDLL("kernel32.dll")
	procReplaceFileW = modkernel32.NewProc("ReplaceFileW")
)

func platformReplace(tmpPath, targetPath string) error {
	replaced, err := syscall.UTF16PtrFromString(targetPath)
	if err != nil {
		return err
	}
	replacement, err := syscall.UTF16PtrFromString(tmpPath)
	if err != nil {
		return err
	}
	// lpBackupFileName NULL: no backup copy; flags 0: default semantics.
	r1, _, callErr := procReplaceFileW.Call(
		uintptr(unsafe.Pointer(replaced)),
		uintptr(unsafe.Pointer(replacement)),
		0,
		0,
		0,
		0,
	)
	if r1 == 0 {
		if callErr != syscall.Errno(0) {
			return callErr
		}
		return os.Rename(tmpPath, targetPath) // fall back when target absent
	}
	return nil
}
