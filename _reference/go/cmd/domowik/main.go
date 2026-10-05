// Command domowik is the thin entry point of the Domowik CLI. All behavior
// lives in internal/cli so it stays importable and testable; this file must
// remain limited to delegation.
package main

import (
	"os"

	"domowik.dev/domowik/internal/cli"
)

func main() {
	os.Exit(cli.Run(os.Args[1:], os.Stdout, os.Stderr))
}
