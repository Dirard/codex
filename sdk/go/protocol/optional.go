package protocol

import (
	"bytes"
	"encoding/json"
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
	return json.Unmarshal(data, &o.value)
}

type OptionalNonNull[T any] struct {
	value T
	set   bool
}

func SomeNonNull[T any](value T) OptionalNonNull[T] {
	return OptionalNonNull[T]{value: value, set: true}
}

func (o OptionalNonNull[T]) IsSet() bool { return o.set }

func (o OptionalNonNull[T]) Value() (T, bool) { return o.value, o.set }

func (o OptionalNonNull[T]) MarshalJSON() ([]byte, error) {
	if !o.set {
		return nil, fmt.Errorf("cannot marshal unset OptionalNonNull directly; generated structs must omit unset fields")
	}
	return json.Marshal(o.value)
}

func (o *OptionalNonNull[T]) UnmarshalJSON(data []byte) error {
	if bytes.Equal(data, []byte("null")) {
		return fmt.Errorf("optional non-null field cannot be null")
	}
	o.set = true
	return json.Unmarshal(data, &o.value)
}
