package release

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"sort"
)

// ReadFileAt resolves a registry-declared relative path against a root and
// returns its bytes.
type ReadFileAt func(rel string) ([]byte, error)

// RenderNotice renders the deterministic NOTICE file from the reviewed
// registry: every entry with noticeRequired contributes a section, sorted by
// component key, embedding each recorded verbatim notice text. It fails
// closed if a required text is missing or does not match its recorded hash;
// a URL reference alone never satisfies a notice obligation.
func RenderNotice(project string, reg *Registry, readFile ReadFileAt) ([]byte, error) {
	var required []Entry
	for _, e := range reg.Entries {
		if e.NoticeRequired && len(e.NoticeFiles) == 0 {
			return nil, fmt.Errorf("entry %q: noticeRequired without any verbatim notice text", e.Key)
		}
		if e.NoticeRequired {
			required = append(required, e)
		}
	}
	sort.Slice(required, func(i, j int) bool { return required[i].Key < required[j].Key })

	var b bytes.Buffer
	fmt.Fprintf(&b, "%s\n", project)
	b.WriteString("Copyright 2026 The Domowik Authors\n")
	b.WriteString("\n")
	b.WriteString("This product includes software developed as part of the Domowik\n")
	b.WriteString("project, licensed under the Apache License, Version 2.0\n")
	b.WriteString("(https://www.apache.org/licenses/LICENSE-2.0).\n")
	for _, e := range required {
		fmt.Fprintf(&b, "\n----- %s (%s) -----\n", e.Key, e.SPDXLicense)
		for i, nf := range e.NoticeFiles {
			data, err := readFile(nf.Path)
			if err != nil {
				return nil, fmt.Errorf("entry %q: notice file %s unreadable: %w", e.Key, nf.Path, err)
			}
			sum := sha256.Sum256(data)
			if got := hex.EncodeToString(sum[:]); got != nf.SHA256 {
				return nil, fmt.Errorf("entry %q: notice file %s does not match its recorded hash; review the upstream text and update the registry deliberately", e.Key, nf.Path)
			}
			if i == 0 {
				b.WriteString("\n")
			} else {
				b.WriteString("\nThe following additional notice applies to portions of this component:\n\n")
			}
			b.Write(ensureTrailingNewline(data))
		}
	}
	return b.Bytes(), nil
}

func ensureTrailingNewline(data []byte) []byte {
	if len(data) > 0 && !bytes.HasSuffix(data, []byte("\n")) {
		return append(data, '\n')
	}
	return data
}
