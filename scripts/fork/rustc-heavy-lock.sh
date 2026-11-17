#!/usr/bin/env bash
# RUSTC_WRAPPER for fork CI test builds: compiles the heaviest test crates one at a time.
#
#   RUSTC_WRAPPER=scripts/fork/rustc-heavy-lock.sh cargo test --no-run ...
#
# cargo runs it as `rustc-heavy-lock.sh RUSTC ARGS...`. A test harness build (`--test`)
# of a crate named in FORK_HEAVY_TEST_CRATES (space-separated crate names) runs under
# an exclusive flock on FORK_HEAVY_TEST_LOCK; every other call runs rustc directly.
# Package and feature selection stay as cargo chose them, so the artifacts are the
# same as without the wrapper.

set -euo pipefail

crate="" harness=0 prev=""
for arg in "${@:2}"; do
  if [[ "${prev}" == "--crate-name" ]]; then
    crate="${arg}"
  fi
  if [[ "${arg}" == "--test" ]]; then
    harness=1
  fi
  prev="${arg}"
done

if [[ "${harness}" -eq 1 && -n "${crate}" && -n "${FORK_HEAVY_TEST_LOCK:-}" ]]; then
  case " ${FORK_HEAVY_TEST_CRATES:-} " in
    *" ${crate} "*) exec flock "${FORK_HEAVY_TEST_LOCK}" "$@" ;;
  esac
fi
exec "$@"
