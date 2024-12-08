#!/usr/bin/env bash
set -eo pipefail

cargo hack check --feature-powerset --depth 1 \
  "${@}"
