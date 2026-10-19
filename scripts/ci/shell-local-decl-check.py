#!/usr/bin/env python3
"""Reject `local a=... b=$a` in the repository's shell scripts.

In `local name="$1" dest="$WORK/$name/x"`, bash expands every right-hand side
in the ENCLOSING scope before any of the locals exists. `$name` on the second
assignment is therefore NOT the value assigned by the first one. It is whatever
`name` meant outside the function:

  * nothing, under `set -u`, which aborts with "name: unbound variable";
  * the CALLER's variable of the same name, silently, when one exists.

The second is the dangerous one. A helper called from a function that happens
to have a local of the same name works correctly, by accident, until it is
called from somewhere that does not -- which is how this shipped in
scripts/ci/macos-bundle-negative-proof.sh and died on its first real run.

Checks `local`, `declare` and `typeset`. Usage:

    shell-local-decl-check.py <file> [<file> ...]

Exits 1 and names every offending line. This is a lint for a specific bug, not
a shell parser: it reads one physical line at a time and does not follow line
continuations, which is sufficient because the construct is written on one line.
"""

import re
import sys

DECL = re.compile(r"^\s*(local|declare|typeset)\s+(.*)$")
# A name being assigned: `name=` at the start of a word.
ASSIGN = re.compile(r"(?:^|\s)([A-Za-z_][A-Za-z0-9_]*)=")
# A name being read: $name or ${name...}
READ = re.compile(r"\$\{?([A-Za-z_][A-Za-z0-9_]*)")


def offences(path):
    found = []
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            match = DECL.match(line)
            if not match:
                continue
            rest = match.group(2)
            if rest.lstrip().startswith("-"):  # flags such as `local -r`
                rest = rest.split(None, 1)[1] if len(rest.split(None, 1)) > 1 else ""
            declared = []
            # Walk the assignments left to right; a read of an already-declared
            # name is the defect.
            for assignment in ASSIGN.finditer(rest):
                name = assignment.group(1)
                value_start = assignment.end()
                next_assignment = ASSIGN.search(rest, value_start)
                value_end = next_assignment.start() if next_assignment else len(rest)
                value = rest[value_start:value_end]
                for read in READ.finditer(value):
                    if read.group(1) in declared:
                        found.append(
                            (number, line.rstrip(), read.group(1), name)
                        )
                declared.append(name)
    return found


def main(argv):
    if len(argv) < 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    total = 0
    for path in argv[1:]:
        for number, text, read_name, into in offences(path):
            total += 1
            print(
                f"{path}:{number}: `${read_name}` is read while assigning "
                f"`{into}` in the same declaration; it resolves in the "
                f"enclosing scope, not to the value just assigned",
                file=sys.stderr,
            )
            print(f"    {text.strip()}", file=sys.stderr)
    if total:
        print(
            f"\n{total} same-statement local declaration(s); split them onto "
            f"separate lines",
            file=sys.stderr,
        )
        return 1
    print(f"shell local-declaration check passed: {len(argv) - 1} file(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
