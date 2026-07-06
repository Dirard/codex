package main

import (
	"fmt"
	"os"
)

func main() {
	fmt.Fprintln(os.Stderr, "protocodex generator command is unavailable before generator support is added")
	os.Exit(2)
}
