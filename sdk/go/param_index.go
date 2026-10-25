package codex

import (
	"encoding/json"
	"strings"
)

// paramIndex finds string fields in a notification's params without decoding them into
// Go values. The router builds one per notification and reuses it for every routing key and
// stream filter, where it used to decode the params into map[string]any several times.
// Lookups match what decoding into map[string]any and walking the path would return:
// exact key match, the last duplicate wins, and only a JSON string is a value. An index is
// immutable, so streams may use it concurrently.
type paramIndex struct {
	members []jsonMember
}

type jsonMember struct {
	key   string
	value []byte
}

// newParamIndex indexes the top-level members of params. Params that are not a valid JSON
// object give an empty index, like a failed decode did.
func newParamIndex(params []byte) *paramIndex {
	index := &paramIndex{}
	if len(params) == 0 || !json.Valid(params) {
		return index
	}
	index.members = objectMembers(params)
	return index
}

// paramIndexFor returns index, or builds one for notification when the caller has none.
func paramIndexFor(index *paramIndex, notification Notification) *paramIndex {
	if index != nil {
		return index
	}
	return newParamIndex(notification.RawParams)
}

// stringAt returns the string at a dot-separated path.
func (p *paramIndex) stringAt(path string) (string, bool) {
	if p == nil || len(p.members) == 0 || path == "" {
		return "", false
	}
	members := p.members
	for {
		segment, rest, nested := strings.Cut(path, ".")
		value, ok := lastMember(members, segment)
		if !ok {
			return "", false
		}
		if !nested {
			return jsonStringValue(value)
		}
		if len(value) == 0 || value[0] != '{' {
			return "", false
		}
		members = objectMembers(value)
		path = rest
	}
}

func lastMember(members []jsonMember, key string) ([]byte, bool) {
	for i := len(members) - 1; i >= 0; i-- {
		if members[i].key == key {
			return members[i].value, true
		}
	}
	return nil, false
}

// objectMembers lists the members of a JSON object as sub-slices of data. data must be
// valid JSON; anything but an object has no members.
func objectMembers(data []byte) []jsonMember {
	i := skipJSONSpace(data, 0)
	if i >= len(data) || data[i] != '{' {
		return nil
	}
	i = skipJSONSpace(data, i+1)
	if i < len(data) && data[i] == '}' {
		return nil
	}
	members := make([]jsonMember, 0, 8)
	for i < len(data) && data[i] == '"' {
		keyEnd := skipJSONString(data, i)
		key := jsonKey(data[i:keyEnd])
		i = skipJSONSpace(data, keyEnd)
		if i >= len(data) || data[i] != ':' {
			return members
		}
		valueStart := skipJSONSpace(data, i+1)
		valueEnd := skipJSONValue(data, valueStart)
		members = append(members, jsonMember{key: key, value: data[valueStart:valueEnd]})
		i = skipJSONSpace(data, valueEnd)
		if i >= len(data) || data[i] != ',' {
			return members
		}
		i = skipJSONSpace(data, i+1)
	}
	return members
}

func skipJSONSpace(data []byte, i int) int {
	for i < len(data) {
		switch data[i] {
		case ' ', '\t', '\n', '\r':
			i++
		default:
			return i
		}
	}
	return i
}

// skipJSONString returns the index after the string that starts at data[i].
func skipJSONString(data []byte, i int) int {
	for j := i + 1; j < len(data); j++ {
		switch data[j] {
		case '\\':
			j++
		case '"':
			return j + 1
		}
	}
	return len(data)
}

// skipJSONValue returns the index after the value that starts at data[i].
func skipJSONValue(data []byte, i int) int {
	if i >= len(data) {
		return i
	}
	switch data[i] {
	case '"':
		return skipJSONString(data, i)
	case '{', '[':
		depth := 0
		for j := i; j < len(data); j++ {
			switch data[j] {
			case '"':
				j = skipJSONString(data, j) - 1
			case '{', '[':
				depth++
			case '}', ']':
				depth--
				if depth == 0 {
					return j + 1
				}
			}
		}
		return len(data)
	default:
		j := i
		for j < len(data) {
			switch data[j] {
			case ',', '}', ']', ' ', '\t', '\n', '\r':
				return j
			}
			j++
		}
		return j
	}
}

// jsonKey decodes a member name; names that need unescaping or UTF-8 repair go through
// encoding/json so they match what it would produce.
func jsonKey(raw []byte) string {
	if plainJSONString(raw) {
		return string(raw[1 : len(raw)-1])
	}
	var key string
	if err := json.Unmarshal(raw, &key); err != nil {
		return ""
	}
	return key
}

func jsonStringValue(raw []byte) (string, bool) {
	if len(raw) == 0 || raw[0] != '"' {
		return "", false
	}
	if plainJSONString(raw) {
		return string(raw[1 : len(raw)-1]), true
	}
	var value string
	if err := json.Unmarshal(raw, &value); err != nil {
		return "", false
	}
	return value, true
}

// plainJSONString reports a quoted ASCII string without escapes, whose bytes are its value.
func plainJSONString(raw []byte) bool {
	if len(raw) < 2 || raw[0] != '"' || raw[len(raw)-1] != '"' {
		return false
	}
	for _, b := range raw[1 : len(raw)-1] {
		if b == '\\' || b >= 0x80 || b < 0x20 {
			return false
		}
	}
	return true
}
