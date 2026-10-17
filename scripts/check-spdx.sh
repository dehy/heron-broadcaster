#!/usr/bin/env sh
# SPDX-License-Identifier: AGPL-3.0-or-later
#
# Fails if a tracked source file lacks the SPDX license header.

set -eu

missing=$(git ls-files \
    '*.rs' '*.ts' '*.tsx' '*.css' '*.sql' '*.sh' \
    '*Dockerfile' 'docker/*.conf' 'docker-compose.yml' '.github/workflows/*.yml' \
    | while read -r file; do
        head -n 5 "$file" | grep -q 'SPDX-License-Identifier: AGPL-3.0-or-later' || echo "$file"
    done)

if [ -n "$missing" ]; then
    echo "Missing SPDX-License-Identifier header:"
    echo "$missing"
    exit 1
fi
echo "All source files carry the SPDX header."
