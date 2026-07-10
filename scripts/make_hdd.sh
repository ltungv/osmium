#!/bin/bash

set -euxo pipefail

dd if=/dev/zero of=hdd.dsk bs=1M count=32

