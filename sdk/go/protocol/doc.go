// Package protocol contains generated Codex app-server protocol types.
//
// Optional and OptionalNonNull fields tell an absent field from an explicit null and a
// value. Their zero value is unset: struct fields of these types are tagged omitzero, and
// a generated MarshalJSON omits them, while MarshalJSON of an unset Optional on its own
// fails. Generated UnmarshalJSON methods follow the app-server's serde rules: required
// fields, nulls, serde aliases and defaults are checked; keys match field names exactly
// (not case-insensitively as with plain encoding/json), and of duplicate keys the last
// one wins.
package protocol
