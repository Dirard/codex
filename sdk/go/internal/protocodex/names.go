package protocodex

import (
	"strconv"
	"strings"
	"unicode"
)

func GoTypeName(input string) string {
	parts := splitIdentifier(input)
	var words []string
	for i, part := range parts {
		if part == "" || isVersionNamespace(part) {
			if len(parts) > 1 && i == 0 {
				continue
			}
		}
		words = append(words, titleWord(part))
	}
	if len(words) == 0 {
		return "Value"
	}
	name := strings.Join(words, "")
	if goReservedWords[name] {
		return name + "Value"
	}
	return name
}

func UniqueGoNames(inputs []string) map[string]string {
	out := make(map[string]string, len(inputs))
	seen := map[string]int{}
	for _, input := range inputs {
		base := GoTypeName(input)
		seen[base]++
		name := base
		if seen[base] > 1 {
			name = base + strconv.Itoa(seen[base])
		}
		out[input] = name
	}
	return out
}

func EnumConstName(typeName, value string) string {
	return GoTypeName(typeName) + GoTypeName(value)
}

func RawMethodName(method string) string {
	return GoTypeName(method)
}

func splitIdentifier(input string) []string {
	var parts []string
	for _, segment := range strings.FieldsFunc(input, func(r rune) bool {
		return !unicode.IsLetter(r) && !unicode.IsDigit(r)
	}) {
		parts = append(parts, splitCamel(segment)...)
	}
	return parts
}

func splitCamel(input string) []string {
	if input == "" {
		return nil
	}
	var parts []string
	start := 0
	runes := []rune(input)
	for i := 1; i < len(runes); i++ {
		prev := runes[i-1]
		cur := runes[i]
		nextStartsWord := i+1 < len(runes) && unicode.IsLower(runes[i+1])
		if (unicode.IsLower(prev) && unicode.IsUpper(cur)) || (unicode.IsUpper(prev) && unicode.IsUpper(cur) && nextStartsWord) {
			parts = append(parts, string(runes[start:i]))
			start = i
		}
	}
	parts = append(parts, string(runes[start:]))
	return parts
}

func titleWord(input string) string {
	if input == "" {
		return ""
	}
	runes := []rune(input)
	runes[0] = unicode.ToUpper(runes[0])
	for i := 1; i < len(runes); i++ {
		runes[i] = unicode.ToLower(runes[i])
	}
	return string(runes)
}

func isVersionNamespace(input string) bool {
	if len(input) < 2 || input[0] != 'v' {
		return false
	}
	for _, r := range input[1:] {
		if !unicode.IsDigit(r) {
			return false
		}
	}
	return true
}

var goReservedWords = map[string]bool{
	"Break":       true,
	"Default":     true,
	"Func":        true,
	"Interface":   true,
	"Select":      true,
	"Case":        true,
	"Defer":       true,
	"Go":          true,
	"Map":         true,
	"Struct":      true,
	"Chan":        true,
	"Else":        true,
	"Goto":        true,
	"Package":     true,
	"Switch":      true,
	"Const":       true,
	"Fallthrough": true,
	"If":          true,
	"Range":       true,
	"Type":        true,
	"Continue":    true,
	"For":         true,
	"Import":      true,
	"Return":      true,
	"Var":         true,
}
