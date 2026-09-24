#!/bin/bash

set -u

wsm="./target/wasm32-wasip1/release-wasi/for-reals2ipc.wasm"

forval="./sample.d/forvalues.bin"

geninput() {
	mkdir -p "./sample.d"

	(
		echo '0000 0000'
		echo '0000 803f'
		echo '0000 0040'
		echo '0000 803f'
	) |
		xxd -r -ps |
		cat >"${forval}"
}

run_wasi() {
	cat /dev/stdin |
		wasmtime \
			run \
			--dir "${PWD}/sample.d::/guest.d" \
			--env ENV_FOR_FILENAME="/guest.d/forvalues.bin" \
			"${wsm}"
}

input1() {
	(

		echo '00 00 00 00'
		echo '01 00 00 00'
		echo '02 01 02 03'
		echo '03 01 02 03'
		echo '04 00 00 00'
		echo '05 00 00 00'
		echo '06 01 02 03'
		echo '07 01 02 03'

		echo 'ff 00 00 00'
		echo 'ff 00 00 00'
		echo 'ff 01 02 03'
		echo 'ff 01 02 03'
		echo 'ff 00 00 00'
		echo 'ff 00 00 00'
		echo 'ff 01 02 03'
		echo 'ff 01 02 03'

	) |
		xxd -r -ps
}

test -f "${forval}" || geninput || exit 1

input1 | run_wasi | xxd | head

which arrow-cat && input1 | run_wasi | arrow-cat
