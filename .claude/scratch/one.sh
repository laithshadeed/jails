#!/bin/bash
# Show one test's panic with a bounded slice of the value it compared.
cd /home/laith/code/jails-merge
cargo test --test cli "$1" 2>&1 | sed -n '/---- .* stdout ----/,/^test result/p' | head -${2:-45}
