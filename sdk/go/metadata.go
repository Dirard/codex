package codex

// Metadata describes the connected runtime and effective SDK configuration.
type Metadata struct {
	RuntimePath                 string
	RuntimeVersion              string
	UserAgent                   string
	ProtocolMode                ProtocolMode
	Compatibility               CompatibilityPolicy
	CompatibilityOverrideActive bool
	CompatibilityNote           string
}
