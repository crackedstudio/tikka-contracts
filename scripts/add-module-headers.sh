#!/usr/bin/env bash
# =============================================================================
# add-module-headers.sh
#
# Checks every .rs file in contracts/ for a module-level doc comment (//!).
# Files that are missing one are listed so you can add the header manually.
#
# Usage: ./scripts/add-module-headers.sh
# =============================================================================

set -euo pipefail

MISSING=()

while IFS= read -r -d '' file; do
    if ! head -1 "$file" | grep -q '^//!'; then
        MISSING+=("$file")
    fi
done < <(find contracts -name '*.rs' -print0)

if [ ${#MISSING[@]} -eq 0 ]; then
    echo "All .rs files have module-level doc comments. ✓"
    exit 0
fi

echo "The following files are missing a //! module doc comment:"
for f in "${MISSING[@]}"; do
    echo "  $f"
done

cat <<'TEMPLATE'

Add this template to the top of each file listed above:

  //! Brief one-line description of what this module does.
  //!
  //! ## Responsibilities
  //! - Responsibility A
  //! - Responsibility B
  //!
  //! ## What does NOT belong here
  //! - Cross-cutting concern X → see `other_module.rs`

TEMPLATE

exit 1
