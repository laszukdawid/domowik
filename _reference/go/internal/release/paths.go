package release

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// mkdirUnderRootMaterializes creates the directory components of rel under
// root one at a time, rejecting any symlinked component before it can be
// followed and never creating anything when a check fails. It returns the
// real (symlink-resolved) absolute parent directory.
func mkdirUnderRootMaterializes(root, rel string) (string, error) {
	realRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return "", fmt.Errorf("resolve repository root: %w", err)
	}
	parts := strings.Split(filepath.ToSlash(filepath.Dir(rel)), "/")
	current := root
	realCurrent := realRoot
	for _, part := range parts {
		if part == "" || part == "." {
			continue
		}
		next := filepath.Join(current, part)
		fi, err := os.Lstat(next)
		switch {
		case os.IsNotExist(err):
			if mkErr := os.Mkdir(next, 0o755); mkErr != nil {
				return "", fmt.Errorf("create directory %q: %w", part, mkErr)
			}
			if fi, err = os.Lstat(next); err != nil {
				return "", fmt.Errorf("stat created directory: %w", err)
			}
		case err != nil:
			return "", fmt.Errorf("stat %q: %w", part, err)
		}
		if fi.Mode()&os.ModeSymlink != 0 {
			return "", fmt.Errorf("directory component %q is a symlink; refusing to follow it", part)
		}
		if !fi.IsDir() {
			return "", fmt.Errorf("directory component %q is not a directory", part)
		}
		current = next
		realNext, err := filepath.EvalSymlinks(next)
		if err != nil {
			return "", fmt.Errorf("resolve %q: %w", part, err)
		}
		if realNext != realCurrent && !strings.HasPrefix(realNext, realCurrent+string(filepath.Separator)) {
			return "", fmt.Errorf("directory component %q resolves outside the repository root", part)
		}
		realCurrent = realNext
		current = next
	}
	return realCurrent, nil
}

// resolveUnderRoot lexically validates rel against root and then resolves the
// full parent chain, requiring the real location to stay under the
// symlink-resolved root. Missing parents are created only when create is
// true, component-wise, so a read path can never materialize directories or
// follow a symlinked parent into creation. It returns the cleaned relative
// slash path plus the real absolute parent directory.
func resolveUnderRoot(root, rel string, create bool) (string, string, error) {
	if err := safeRelPath(rel); err != nil {
		return "", "", err
	}
	realRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return "", "", fmt.Errorf("resolve repository root: %w", err)
	}
	full := filepath.Join(root, filepath.FromSlash(rel))
	dir := filepath.Dir(full)

	var realDir string
	if create {
		realDir, err = mkdirUnderRootMaterializes(root, rel)
	} else {
		realDir, err = filepath.EvalSymlinks(dir)
		if os.IsNotExist(err) {
			return "", "", fmt.Errorf("path %q does not exist", rel)
		}
	}
	if err != nil {
		return "", "", err
	}
	if realDir != realRoot && !strings.HasPrefix(realDir, realRoot+string(filepath.Separator)) {
		return "", "", fmt.Errorf("output path %q resolves outside the repository root", rel)
	}
	relReal, err := filepath.Rel(realRoot, realDir)
	if err != nil {
		return "", "", fmt.Errorf("relativize output directory: %w", err)
	}
	return filepath.ToSlash(filepath.Join(relReal, filepath.Base(full))), realDir, nil
}

// rejectSymlinkTarget fails if the final path element exists as a symlink,
// so reads and writes never traverse a hostile final entry.
func rejectSymlinkTarget(dir, base string) error {
	fi, err := os.Lstat(filepath.Join(dir, base))
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("stat output target: %w", err)
	}
	if fi.Mode()&os.ModeSymlink != 0 {
		return fmt.Errorf("target %q is a symlink; refusing to follow it", base)
	}
	return nil
}

// writeFileUnderRoot writes data to rel under root after full parent-chain
// and final-target symlink checks.
func writeFileUnderRoot(root, rel string, data []byte) error {
	_, realDir, err := resolveUnderRoot(root, rel, true)
	if err != nil {
		return err
	}
	base := filepath.Base(filepath.FromSlash(rel))
	if err := rejectSymlinkTarget(realDir, base); err != nil {
		return fmt.Errorf("write %s: %w", rel, err)
	}
	full := filepath.Join(realDir, base)
	if err := os.WriteFile(full, data, 0o644); err != nil {
		return fmt.Errorf("write %s: %w", rel, err)
	}
	return nil
}

// readFileUnderRoot reads rel under root after full parent-chain and
// final-target symlink checks.
func readFileUnderRoot(root, rel string) ([]byte, error) {
	cleanRel, realDir, err := resolveUnderRoot(root, rel, false)
	if err != nil {
		return nil, err
	}
	base := filepath.Base(filepath.FromSlash(cleanRel))
	if err := rejectSymlinkTarget(realDir, base); err != nil {
		return nil, fmt.Errorf("read %s: %w", rel, err)
	}
	return os.ReadFile(filepath.Join(realDir, base))
}

func readFileUnderRootFn(root string) ReadFileAt {
	return func(rel string) ([]byte, error) { return readFileUnderRoot(root, rel) }
}
