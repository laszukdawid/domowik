package main

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"
)

// repoRoot returns the absolute root of the enclosing Git checkout.
func repoRoot() (string, error) {
	out, err := exec.Command("git", "rev-parse", "--show-toplevel").Output()
	if err != nil {
		return "", fmt.Errorf("resolve repository root: %w", err)
	}
	root := strings.TrimSpace(string(out))
	if root == "" {
		return "", fmt.Errorf("repository root is empty")
	}
	return root, nil
}

// gitHeadTime returns the HEAD commit time as unix seconds.
func gitHeadTime(root string) (int64, error) {
	out, err := exec.Command("git", "-C", root, "log", "-1", "--format=%ct").Output()
	if err != nil {
		return 0, fmt.Errorf("read HEAD commit time: %w", err)
	}
	sec, err := strconv.ParseInt(strings.TrimSpace(string(out)), 10, 64)
	if err != nil {
		return 0, fmt.Errorf("parse HEAD commit time: %w", err)
	}
	return sec, nil
}
