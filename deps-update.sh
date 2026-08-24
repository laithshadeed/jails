#!/usr/bin/env bash
# Clone/update the read-only upstream source checkouts listed in deps.tsv.
#
#   ./update.sh                 # clone what's missing, fast-forward the rest
#   ./update.sh kafka netty     # only those (substring match on the dir name)
#   ./update.sh --list          # show manifest + local state, touch nothing
#   ./update.sh --clone-only    # clone missing, don't touch existing
#   ./update.sh -j 8            # parallelism (default 4)
#
# New clones are blobless (--filter=blob:none): full history, blobs fetched on
# demand. The existing full clones are left as they are -- 6+ GB of them.
# A checkout with local changes or a detached HEAD is skipped, never reset.
set -uo pipefail

# The manifest sits beside this script at the repository root; the checkouts
# do not -- they live in `deps/`, which is gitignored. Those were one
# directory for as long as `deps/deps.tsv` and `deps/update.sh` existed, and
# when they moved up here the clone target came with them: one run cloned all
# 81 repositories into the repository *root*, where `.gitignore`'s `/deps/`
# does not match them and `git add -A` files each one as a gitlink. So the two
# paths are now named separately, and neither is "wherever this script is".
cd "$(dirname "$(readlink -f "$0")")" || exit 1
MANIFEST=deps.tsv
CHECKOUTS=deps
mkdir -p "$CHECKOUTS" || exit 1
REMOTE_PREFIX=${JAILS_DEPS_REMOTE:-github-personal:}
JOBS=4
LIST=0
CLONE_ONLY=0
FILTERS=()

while (($#)); do
  case $1 in
    --list|-l)       LIST=1 ;;
    --clone-only)    CLONE_ONLY=1 ;;
    -j)              JOBS=$2; shift ;;
    -j*)             JOBS=${1#-j} ;;
    -h|--help)       sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*)              echo "update.sh: unknown flag $1" >&2; exit 2 ;;
    *)               FILTERS+=("$1") ;;
  esac
  shift
done

if [[ ! -f $MANIFEST ]]; then
  echo "update.sh: $PWD/$MANIFEST not found" >&2; exit 1
fi

wanted() {                                    # $1 = dir name
  ((${#FILTERS[@]})) || return 0
  local f
  for f in "${FILTERS[@]}"; do [[ $1 == *"$f"* ]] && return 0; done
  return 1
}

# --- read the manifest -------------------------------------------------------
DIRS=() REPOS=()
while IFS=$'\t' read -r dir repo _; do
  [[ -z ${dir// } || $dir == \#* ]] && continue
  wanted "$dir" || continue
  DIRS+=("$dir"); REPOS+=("$repo")
done < "$MANIFEST"

if ((${#DIRS[@]} == 0)); then
  echo "update.sh: nothing in $MANIFEST matched" >&2; exit 1
fi

if ((LIST)); then
  printf '%-38s %-42s %s\n' DIR REPO STATE
  for i in "${!DIRS[@]}"; do
    d=${CHECKOUTS}/${DIRS[i]}
    if [[ -d $d/.git ]]; then
      state="$(git -C "$d" rev-parse --abbrev-ref HEAD 2>/dev/null) @ $(git -C "$d" log -1 --format=%cs 2>/dev/null)"
      [[ -n $(git -C "$d" status --porcelain 2>/dev/null) ]] && state+=" (dirty)"
    elif [[ -e $d ]]; then
      state="not a git repo"
    else
      state="missing"
    fi
    printf '%-38s %-42s %s\n' "${DIRS[i]}" "${REPOS[i]}" "$state"
  done
  exit 0
fi

# First interesting line of a git error, for a one-line report.
# git prints the interesting half of a remote failure on the line *after*
# "fatal: remote error:", so collapse the whole thing to one line rather than
# reporting a bare, useless "remote error:".
err_line() {
  tr '\n' ' ' <<<"$1" | tr -s ' ' | sed 's/^ *//; s/ *$//' | cut -c1-160
}

# --- one checkout ------------------------------------------------------------
# Prints "STATUS<TAB>dir<TAB>detail". Runs under xargs, so it must be a
# standalone invocation of this script.
one() {
  local name=$1 repo=$2
  # The checkout's path, never a bare name: every `git` call below would
  # otherwise act on `$PWD/<name>`, which is the repository root.
  local dir="${CHECKOUTS}/${name}"
  # Separate statement on purpose: bash declares every name in a `local` before
  # running its assignments, so a `url=...${repo}...` sharing the line above
  # would expand $repo as the blanked local and ask GitHub to clone "".
  local url="${REMOTE_PREFIX}${repo}.git"

  if [[ ! -e $dir ]]; then
    # A dropped connection mid-clone is common on the big repos; retry rather
    # than leaving a half-clone behind.
    local attempt out
    for attempt in 1 2 3; do
      if out=$(git clone --quiet --filter=blob:none "$url" "$dir" 2>&1); then
        printf 'CLONED\t%s\t%s\n' "$name" "$repo"; return
      fi
      rm -rf "$dir"
      sleep $((attempt * 5))
    done
    printf 'FAILED\t%s\t%s\n' "$name" "clone: $(err_line "$out")"
    return
  fi

  [[ -d $dir/.git ]] || { printf 'SKIP\t%s\tnot a git repo\n' "$name"; return; }
  ((CLONE_ONLY)) && { printf 'SKIP\t%s\talready present\n' "$name"; return; }

  local branch
  branch=$(git -C "$dir" symbolic-ref --quiet --short HEAD) \
    || { printf 'SKIP\t%s\tdetached HEAD\n' "$name"; return; }
  [[ -n $(git -C "$dir" status --porcelain) ]] \
    && { printf 'SKIP\t%s\tlocal changes\n' "$name"; return; }

  local before after out
  before=$(git -C "$dir" rev-parse HEAD)
  if ! out=$(git -C "$dir" fetch --quiet --prune origin 2>&1); then
    printf 'FAILED\t%s\t%s\n' "$name" "fetch: $(err_line "$out")"; return
  fi
  if ! out=$(git -C "$dir" merge --ff-only --quiet "@{upstream}" 2>&1); then
    printf 'SKIP\t%s\t%s\n' "$name" "not fast-forwardable ($branch)"; return
  fi
  after=$(git -C "$dir" rev-parse HEAD)
  if [[ $before == "$after" ]]; then
    printf 'CURRENT\t%s\t%s\n' "$name" "$branch"
  else
    printf 'UPDATED\t%s\t%s\n' "$name" \
      "$branch +$(git -C "$dir" rev-list --count "$before..$after")"
  fi
}

# --- drive, at most $JOBS at a time -------------------------------------------
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
echo "updating ${#DIRS[@]} checkout(s) with $JOBS job(s)..."
for i in "${!DIRS[@]}"; do
  while (( $(jobs -rp | wc -l) >= JOBS )); do wait -n; done
  one "${DIRS[i]}" "${REPOS[i]}" > "$tmp/$i" &
done
wait

fail=0
for i in "${!DIRS[@]}"; do
  IFS=$'\t' read -r status dir detail < "$tmp/$i"
  case $status in
    FAILED) fail=1 ;;
    CURRENT) ((${#FILTERS[@]})) || continue ;;   # quiet unless asked for
  esac
  printf '%-8s %-38s %s\n' "$status" "$dir" "$detail"
done

if ((fail)); then
  echo "one or more checkouts failed" >&2
  exit 1
fi
echo "done."
