package cli

import (
	"fmt"
	"io"

	"domowik.dev/domowik/internal/migration"
	"domowik.dev/domowik/internal/persist"
)

// migrate.go registers the issue-0031 `migrate` subcommand through the
// isolated registration seam. Everything this command does lives here;
// the shared scaffold dispatch in cli.go only consults the registry.

func init() {
	registerSubcommand("migrate", runMigrate)
}

var subcommands = map[string]func(args []string, stdout, stderr io.Writer) int{}

func registerSubcommand(name string, fn func([]string, io.Writer, io.Writer) int) {
	subcommands[name] = fn
}

func runSubcommand(args []string, stdout, stderr io.Writer) (int, bool) {
	if len(args) > 0 {
		if fn, ok := subcommands[args[0]]; ok {
			return fn(args[1:], stdout, stderr), true
		}
	}
	return 0, false
}

const migrateUsage = `usage: domowik migrate SOURCE [--to TARGET] [--dry-run]

Migrate a .domo package to the schema this build emits (FORMAT-SPEC §8.1 V1).
The migrated package is written as a complete new file through the atomic
save machine; the original survives until migrated output validates and
replaces atomically. Unknown optional data is preserved or the write is
refused; nothing is silently stripped.

  --to TARGET   write migrated output to TARGET instead of in place
  --dry-run     validate and report source/target schema; write nothing
`

func runMigrate(args []string, stdout, stderr io.Writer) int {
	var source, target string
	dryRun := false
	for i := 0; i < len(args); i++ {
		switch args[i] {
		case "--dry-run":
			dryRun = true
		case "--help", "-h":
			fmt.Fprint(stdout, migrateUsage)
			return 0
		case "--to":
			i++
			if i >= len(args) {
				fmt.Fprint(stderr, "domowik migrate: --to requires a path\n\n")
				fmt.Fprint(stderr, migrateUsage)
				return 2
			}
			target = args[i]
		default:
			if len(args[i]) > 0 && args[i][0] == '-' {
				fmt.Fprintf(stderr, "domowik migrate: unknown flag %q\n\n", args[i])
				fmt.Fprint(stderr, migrateUsage)
				return 2
			}
			if source != "" {
				fmt.Fprint(stderr, "domowik migrate: exactly one SOURCE package is accepted\n\n")
				fmt.Fprint(stderr, migrateUsage)
				return 2
			}
			source = args[i]
		}
	}
	if source == "" {
		fmt.Fprint(stderr, "domowik migrate: SOURCE package required\n\n")
		fmt.Fprint(stderr, migrateUsage)
		return 2
	}

	rep, res, err := migration.Run(source, migration.Options{Target: target, DryRun: dryRun})
	if err != nil {
		if c, ok := persist.CategoryOf(err); ok {
			fmt.Fprintf(stderr, "domowik migrate: %s: %v\n", c, err)
		} else {
			fmt.Fprintf(stderr, "domowik migrate: %v\n", err)
		}
		return 1
	}
	action := "migrated"
	switch {
	case dryRun:
		action = "dry-run (no write performed)"
	case rep.AlreadyAtTarget:
		action = "no change (already at target schema)"
	}
	fmt.Fprint(stdout, rep.Render(action))
	if res != nil && res.DurabilityCaveat {
		fmt.Fprintln(stdout, "durability caveat: replacement completed; directory sync failed or unsupported (F6)")
	}
	return 0
}
