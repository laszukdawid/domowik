package cli

import (
	"bytes"
	"testing"
)

func TestRun(t *testing.T) {
	tests := []struct {
		name       string
		args       []string
		wantCode   int
		wantStdout string
		wantStderr string
	}{
		{
			name:       "version flag exits zero on stdout",
			args:       []string{"--version"},
			wantCode:   0,
			wantStdout: "domowik version 0.0.0-provisional\n",
		},
		{
			name:       "version subcommand exits zero on stdout",
			args:       []string{"version"},
			wantCode:   0,
			wantStdout: "domowik version 0.0.0-provisional\n",
		},
		{
			name:       "help flag exits zero on stdout",
			args:       []string{"--help"},
			wantCode:   0,
			wantStdout: usage,
		},
		{
			name:       "help subcommand exits zero on stdout",
			args:       []string{"help"},
			wantCode:   0,
			wantStdout: usage,
		},
		{
			name:       "no arguments prints usage on stderr only",
			args:       nil,
			wantCode:   2,
			wantStderr: usage,
		},
		{
			name:       "unknown command reports on stderr only",
			args:       []string{"frobnicate"},
			wantCode:   2,
			wantStderr: "domowik: unknown command or flag \"frobnicate\"\n\n" + usage,
		},
		{
			name:       "unknown flag reports on stderr only",
			args:       []string{"--bogus"},
			wantCode:   2,
			wantStderr: "domowik: unknown command or flag \"--bogus\"\n\n" + usage,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var stdout, stderr bytes.Buffer
			got := Run(tt.args, &stdout, &stderr)
			if got != tt.wantCode {
				t.Errorf("Run(%q) exit code = %d, want %d", tt.args, got, tt.wantCode)
			}
			if stdout.String() != tt.wantStdout {
				t.Errorf("Run(%q) stdout = %q, want %q", tt.args, stdout.String(), tt.wantStdout)
			}
			if stderr.String() != tt.wantStderr {
				t.Errorf("Run(%q) stderr = %q, want %q", tt.args, stderr.String(), tt.wantStderr)
			}
		})
	}
}

func TestStreamsNeverMixed(t *testing.T) {
	cases := [][]string{
		nil,
		{"frobnicate"},
		{"--bogus"},
	}
	for _, args := range cases {
		var stdout, stderr bytes.Buffer
		Run(args, &stdout, &stderr)
		if stderr.Len() > 0 && stdout.Len() > 0 {
			t.Errorf("Run(%q) mixed streams: stdout %q, stderr %q", args, stdout.String(), stderr.String())
		}
	}
}
