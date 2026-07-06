package codex

// ConfigError reports invalid SDK configuration before startup.
type ConfigError struct {
	Reason string
}

func (e *ConfigError) Error() string {
	return "codex sdk config error: " + e.Reason
}
