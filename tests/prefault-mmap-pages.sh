#!/bin/bash

set -euo pipefail

declare ROOT=$PWD
declare QDRANT_DIR=$ROOT/$1
declare BFB_DIR=$ROOT/$2

if [[ ! -d $QDRANT_DIR ]]
then
	echo "$QDRANT_DIR is not a directory or does not exist" >&2
	exit 1
fi

if [[ ! -d $BFB_DIR ]]
then
	echo "$BFB_DIR is not a directory or does not exist" >&2
	exit 2
fi

cd $QDRANT_DIR
cargo build --release --bin qdrant

cd $BFB_DIR
cargo build --release

cd $QDRANT_DIR

QDRANT__LOG_LEVEL=debug,raft=info,segment::common::mmap_ops=trace \
QDRANT__STORAGE__OPTIMIZERS__MEMMAP_THRESHOLD_KB=1 \
./target/release/qdrant &

$BFB_DIR/target/release/bfb -n 1000000 --indexing-threshold 1000000000

kill %%
wait

QDRANT__LOG_LEVEL=debug,raft=info,segment::common::mmap_ops=trace \
QDRANT__STORAGE__OPTIMIZERS__MEMMAP_THRESHOLD_KB=1 \
./target/release/qdrant &

function search() {
	time \
		curl localhost:6333/collections/benchmark/points/search \
		-X POST -H 'Content-Type: application/json' --data-raw '{
			"vector": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
			"limit": 10,
			"with_vectors": false,
			"with_payload": true
		}'
}

search
search
