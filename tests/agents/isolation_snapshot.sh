isolation_snapshot() {
  (
    cd "$1"
    find . -path ./vault/.git -prune -o -print | sort | while IFS= read -r path; do
      [ -d "$path" ] && echo "$path/" || echo "$path: $(cksum < "$path")"
    done
    git -C "$1/vault" --no-optional-locks status --porcelain
    git -C "$1/vault" rev-parse HEAD
  )
}
