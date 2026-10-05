// Package cli implements the Domowik command-line entry surface.
//
// It is a provisional scaffold placeholder, not the Phase 1 CLI described in
// the roadmap: it establishes a tested package boundary with exact stream
// and exit-code behavior so later subcommands (issues 0014-0017, 0027) can
// extend it without changing its contract.
package cli

import (
	"fmt"
	"io"
	"strings"
)

// version is provisional and carries no product-release meaning. It is
// overridable at build time only via -ldflags -X; there is deliberately no
// VCS, file, or environment fallback.
var version = "0.0.0-provisional"

const usage = `domowik is a scaffold placeholder, not the planned CLI.

Usage:
  domowik --version   print the provisional version
  domowik version     same as --version
  domowik --help      show this help
  domowik help        same as --help
  domowik ops FILE    report retained batch framing without executing (JSON)
  domowik compact FILE fold the retained log into the snapshot, in place

The asset-store surface of issue 0026 is implemented here:
  domowik asset add|list|verify|remove|pin|unpin|gc
  domowik pack (--thin|--portable) INPUT OUTPUT

Document, geometry, and export commands are specified separately and are
not implemented here.
`

// Run executes the command surface against args (without the program
// name) and returns the process exit code: 0 for success, 1 for an
// operational failure reported by a subcommand, 2 for a usage error.
// Success output goes only to stdout; diagnostics, errors, and
// usage-on-failure go only to stderr. The two streams are never mixed
// within a single invocation.
func Run(args []string, stdout, stderr io.Writer) int {
	switch {
	case len(args) == 1 && (args[0] == "--version" || args[0] == "version"):
		fmt.Fprintf(stdout, "domowik version %s\n", version)
		return 0
	case len(args) >= 1 && (args[0] == "--help" || args[0] == "help"):
		fmt.Fprint(stdout, usage)
		return 0
	case len(args) >= 1 && args[0] == "ops":
		return runOps(args[1:], stdout, stderr)
	case len(args) >= 1 && args[0] == "compact":
		return runCompact(args[1:], stdout, stderr)
	case len(args) == 0:
		fmt.Fprint(stderr, usage)
		return 2
	case len(args) > 0 && args[0] == "apply":
		return runApply(args[1:], stdout, stderr)
	case args[0] == "asset":
		return runAsset(args[1:], stdout, stderr)
	case args[0] == "pack":
		return runPack(args[1:], stdout, stderr)
	default:
		if code, handled := runSubcommand(args, stdout, stderr); handled {
			return code
		}
		fmt.Fprintf(stderr, "domowik: unknown command or flag %q\n\n", strings.Join(args, " "))
		fmt.Fprint(stderr, usage)
		return 2
	}
}
