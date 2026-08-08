package protocodex

type ServerHandlerMapping struct {
	Method       string
	HandlerOwner string
	Visibility   string
	Capability   string
}

func mapServerRequests(entries []ServerRequestEntry) map[string]ServerRequestEntry {
	byMethod := make(map[string]ServerRequestEntry, len(entries))
	for _, entry := range entries {
		byMethod[entry.Method] = entry
	}
	return byMethod
}
