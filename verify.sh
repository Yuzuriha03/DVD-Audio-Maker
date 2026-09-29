#!/usr/bin/env bash
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENVSH="$HERE/../local-bin/env.sh"
if [ -f "$ENVSH" ]; then . "$ENVSH"; fi
WHAT="${1:-all}"
if [ "$#" -gt 0 ]; then shift; fi
exec dotnet run --project "$HERE/src/DvdaMaker.Cli/DvdaMaker.Cli.csproj" -- verify "$WHAT" "$@"
