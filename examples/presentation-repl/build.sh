#!/bin/sh
set -eu
cd "$(dirname "$0")"
exec "${BIRNAM:-birnam}" build .
