#!/usr/bin/env bash
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENVSH="$HERE/../local-bin/env.sh"
if [ -f "$ENVSH" ]; then . "$ENVSH"; fi

PROJECT="$HERE/src/DvdaMaker.Cli/DvdaMaker.Cli.csproj"

if [ "${1:-}" = "--config" ] && [ "$#" -eq 1 ]; then
	exec dotnet run --project "$PROJECT" -- config
fi

prepare_args=()
build_args=()
while [ "$#" -gt 0 ]; do
	case "$1" in
		--dry-run)
			build_args+=("$1")
			shift
			;;
		--config)
			if [ "$#" -lt 2 ]; then
				echo "[配置错误] --config 后需要路径" >&2
				exit 2
			fi
			prepare_args+=("--config" "$2")
			build_args+=("--config" "$2")
			shift 2
			;;
		*)
			echo "[错误] build.sh 未知参数: $1" >&2
			exit 2
			;;
	esac
done

dotnet run --project "$PROJECT" -- prepare "${prepare_args[@]}"
exec dotnet run --project "$PROJECT" -- build "${build_args[@]}"
