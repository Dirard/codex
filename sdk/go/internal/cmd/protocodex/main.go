package main

import (
	"flag"
	"fmt"
	"os"

	"github.com/openai/codex/sdk/go/internal/protocodex"
)

func main() {
	var opts protocodex.GenerateOptions
	flag.StringVar(&opts.Mode, "mode", "both", "generation mode: stable, experimental, or both")
	flag.StringVar(&opts.StableSchemaRoot, "stable-schema-root", "", "stable schema root containing json/")
	flag.StringVar(&opts.ExperimentalSchemaRoot, "experimental-schema-root", "", "experimental schema root containing json/")
	flag.StringVar(&opts.ManifestPath, "manifest", "", "Go SDK protocol manifest path")
	flag.StringVar(&opts.OutDir, "out", "", "protocol package output directory")
	flag.StringVar(&opts.RootOutDir, "root-out", "", "root SDK package output directory")
	flag.BoolVar(&opts.Check, "check", false, "check generated output without writing")
	flag.Parse()

	if opts.StableSchemaRoot == "" || opts.ExperimentalSchemaRoot == "" || opts.ManifestPath == "" {
		fmt.Fprintln(os.Stderr, "--stable-schema-root, --experimental-schema-root, and --manifest are required")
		os.Exit(2)
	}
	if err := protocodex.Generate(opts); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
