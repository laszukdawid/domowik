package release

import (
	"fmt"
	"os"
	"strconv"
	"time"
)

// EpochSource names how the SBOM creation timestamp was determined.
type EpochSource string

const (
	// EpochFromEnv means SOURCE_DATE_EPOCH was present and valid.
	EpochFromEnv EpochSource = "SOURCE_DATE_EPOCH"
	// EpochFromHead means the timestamp was derived immutably from the HEAD
	// commit time of the repository being inventoried.
	EpochFromHead EpochSource = "git-head-commit-time"
)

// Getenv reads an environment variable; tests substitute it.
type Getenv func(string) (string, bool)

// HeadTime returns the repository's HEAD commit time as unix seconds; tests
// substitute it.
type HeadTime func() (int64, error)

// ResolveEpoch determines the deterministic creation timestamp. Policy:
//
//   - If SOURCE_DATE_EPOCH is set it must be a decimal unix-seconds value;
//     anything else fails closed rather than falling back silently.
//   - Otherwise the epoch is derived immutably from the HEAD commit time, so
//     a given commit always produces the same timestamp without operator
//     input. Wall-clock time is never used.
func ResolveEpoch(getenv Getenv, headTime HeadTime) (time.Time, EpochSource, error) {
	if v, ok := getenv("SOURCE_DATE_EPOCH"); ok {
		sec, err := strconv.ParseInt(v, 10, 64)
		if err != nil || sec < 0 {
			return time.Time{}, "", fmt.Errorf("SOURCE_DATE_EPOCH must be non-negative unix seconds, got %q", v)
		}
		return time.Unix(sec, 0).UTC(), EpochFromEnv, nil
	}
	sec, err := headTime()
	if err != nil {
		return time.Time{}, "", fmt.Errorf("derive epoch: %w", err)
	}
	return time.Unix(sec, 0).UTC(), EpochFromHead, nil
}

// EnvGetenv adapts os.LookupEnv to Getenv.
func EnvGetenv(key string) (string, bool) {
	return os.LookupEnv(key)
}
