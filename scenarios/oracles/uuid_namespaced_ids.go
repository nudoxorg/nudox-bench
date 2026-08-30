// Oracle for `go/uuid/implement-namespaced-ids`.
//
// A typical, working use of github.com/google/uuid's hash-based (v5) and
// random (v4) constructors, plus parsing. Written against the package's own
// declarations in uuid.go, version4.go and hash.go of v1.6.0.

package oracle

import (
	"fmt"

	"github.com/google/uuid"
)

// StableID derives the same UUID every time for a given name within a
// namespace, using SHA-1 (version 5).
func StableID(namespace uuid.UUID, name string) uuid.UUID {
	return uuid.NewSHA1(namespace, []byte(name))
}

// DNSID derives a stable id under the well-known DNS namespace.
func DNSID(host string) uuid.UUID {
	return uuid.NewSHA1(uuid.NameSpaceDNS, []byte(host))
}

// RandomID returns a fresh random (version 4) UUID, propagating the error
// rather than panicking.
func RandomID() (uuid.UUID, error) {
	return uuid.NewRandom()
}

// MustFromString parses a UUID and panics on failure — for global variable
// initialization.
func MustFromString(s string) uuid.UUID {
	return uuid.MustParse(s)
}

// FromString parses a UUID, returning an error the caller can handle.
func FromString(s string) (uuid.UUID, error) {
	parsed, err := uuid.Parse(s)
	if err != nil {
		return uuid.Nil, fmt.Errorf("parsing %q: %w", s, err)
	}
	return parsed, nil
}
