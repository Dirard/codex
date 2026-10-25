package protocol

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
)

type Optional[T any] struct {
	value T
	set   bool
	null  bool
}

func Some[T any](value T) Optional[T] {
	return Optional[T]{value: value, set: true}
}

func Null[T any]() Optional[T] {
	return Optional[T]{set: true, null: true}
}

func (o Optional[T]) IsSet() bool { return o.set }

func (o Optional[T]) IsNull() bool { return o.set && o.null }

// IsZero reports whether o is unset, so that a struct field tagged omitzero omits it.
func (o Optional[T]) IsZero() bool { return !o.set }

func (o Optional[T]) Value() (T, bool) {
	return o.value, o.set && !o.null
}

func (o Optional[T]) MarshalJSON() ([]byte, error) {
	if !o.set {
		return nil, fmt.Errorf("cannot marshal unset Optional directly; generated structs must omit unset fields")
	}
	if o.null {
		return []byte("null"), nil
	}
	return json.Marshal(o.value)
}

func (o *Optional[T]) UnmarshalJSON(data []byte) error {
	o.set = true
	if bytes.Equal(data, []byte("null")) {
		o.null = true
		var zero T
		o.value = zero
		return nil
	}
	o.null = false
	return unmarshalOptionalValue(data, &o.value)
}

type OptionalNonNull[T any] struct {
	value T
	set   bool
}

func SomeNonNull[T any](value T) OptionalNonNull[T] {
	return OptionalNonNull[T]{value: value, set: true}
}

func (o OptionalNonNull[T]) IsSet() bool { return o.set }

// IsZero reports whether o is unset, so that a struct field tagged omitzero omits it.
func (o OptionalNonNull[T]) IsZero() bool { return !o.set }

func (o OptionalNonNull[T]) Value() (T, bool) { return o.value, o.set }

func (o OptionalNonNull[T]) MarshalJSON() ([]byte, error) {
	if !o.set {
		return nil, fmt.Errorf("cannot marshal unset OptionalNonNull directly; generated structs must omit unset fields")
	}
	return json.Marshal(o.value)
}

var errOptionalNonNullNull = errors.New("optional non-null field cannot be null")

func (o *OptionalNonNull[T]) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return errOptionalNonNullNull
	}
	o.set = true
	return unmarshalOptionalValue(data, &o.value)
}

// unmarshalOptionalValue decodes an optional value. A string is decoded directly from
// the literal: decodeJSONString accepts only valid literals, and json.Unmarshal would
// validate and scan the value twice more.
func unmarshalOptionalValue[T any](data []byte, value *T) error {
	if text, ok := any(value).(*string); ok {
		if decoded, ok := decodeJSONString(data); ok {
			*text = decoded
			return nil
		}
	}
	return json.Unmarshal(data, value)
}

// The helpers below normalize a copy of a generated struct before it is marshaled, for
// serde skip_serializing_if predicates that an omitzero tag alone cannot express.

// withoutNull drops an explicit null (skip_serializing_if = "Option::is_none").
func (o Optional[T]) withoutNull() Optional[T] {
	if o.null {
		return Optional[T]{}
	}
	return o
}

// optionalWithoutFalse drops false (skip_serializing_if = "std::ops::Not::not").
func optionalWithoutFalse[B ~bool](o Optional[B]) Optional[B] {
	if value, ok := o.Value(); ok && !bool(value) {
		return Optional[B]{}
	}
	return o
}

func nonNullWithoutFalse[B ~bool](o OptionalNonNull[B]) OptionalNonNull[B] {
	if value, ok := o.Value(); ok && !bool(value) {
		return OptionalNonNull[B]{}
	}
	return o
}

// optionalWithoutEmpty drops an empty or null slice (skip_serializing_if = "Vec::is_empty").
func optionalWithoutEmpty[S ~[]E, E any](o Optional[S]) Optional[S] {
	if value, ok := o.Value(); !ok || len(value) == 0 {
		return Optional[S]{}
	}
	return o
}

func nonNullWithoutEmpty[S ~[]E, E any](o OptionalNonNull[S]) OptionalNonNull[S] {
	if value, ok := o.Value(); !ok || len(value) == 0 {
		return OptionalNonNull[S]{}
	}
	return o
}
