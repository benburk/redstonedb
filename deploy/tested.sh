#!/bin/sh
# The files the test suite last passed on, as a git tree: `mark` notes the
# working tree's, untracked files included and ignored ones not, and `head`
# succeeds when they are exactly the committed HEAD's, so deploying it need
# not run the suite again.
set -eu
stamp=target/tested-tree
tree() {
    dir=$(mktemp -d)
    trap 'rm -rf "$dir"' EXIT
    GIT_INDEX_FILE=$dir/index git read-tree HEAD
    GIT_INDEX_FILE=$dir/index git add -A
    GIT_INDEX_FILE=$dir/index git write-tree
}
case "${1:-}" in
mark)
    mkdir -p target
    tree >"$stamp"
    ;;
head)
    [ "$(cat "$stamp" 2>/dev/null)" = "$(git rev-parse 'HEAD^{tree}')" ]
    echo "The suite already passed on $(git rev-parse --short HEAD)."
    ;;
*)
    echo "usage: $0 mark|head" >&2
    exit 2
    ;;
esac
