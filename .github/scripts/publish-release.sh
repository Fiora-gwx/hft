#!/usr/bin/env bash
# Publish a GitHub Release from the files in a directory
# (docs/development.md#发布工作流release-aarch64yml step 5; release-x86-cross.yml uses it too).
#
# Environment: TAG, SHA, CHANNEL (release|dev), CREATE_TAG (true|false), DIST (directory),
# NOTES (release notes), GH_TOKEN (contents: write), GITHUB_REPOSITORY.
# With CREATE_TAG=true an annotated tag is created on SHA and pushed first (a tag pushed with
# GITHUB_TOKEN does not trigger workflows). Formal release assets are immutable: an existing
# Release for TAG is an error. For dev, pre-releases beyond the newest 10 are deleted (tags kept).
set -euo pipefail

: "${TAG:?}" "${SHA:?}" "${CHANNEL:?}" "${DIST:?}"
repo=$GITHUB_REPOSITORY

if [[ ${CREATE_TAG:-false} == true ]]; then
  git config user.name "github-actions[bot]"
  git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
  git tag -a "$TAG" "$SHA" -m "$TAG (development build of $SHA)"
  git push origin "refs/tags/$TAG"
fi

if gh release view "$TAG" --repo "$repo" >/dev/null 2>&1; then
  echo "::error::Release $TAG already exists; release assets are immutable"
  exit 1
fi

mapfile -t files < <(find "$DIST" -maxdepth 1 -type f | sort)
((${#files[@]})) || { echo "::error::nothing to publish in $DIST"; exit 1; }
args=(--repo "$repo" --verify-tag --title "$TAG" --notes "${NOTES:-$TAG ($SHA)}")
[[ $CHANNEL == dev ]] && args+=(--prerelease)
gh release create "$TAG" "${files[@]}" "${args[@]}"

if [[ $CHANNEL == dev ]]; then
  gh release list --repo "$repo" --limit 1000 --json tagName,isPrerelease,createdAt \
    --jq '[.[] | select(.isPrerelease and (.tagName | test("-dev\\.[0-9]+$")))]
          | sort_by(.createdAt) | reverse | .[10:] | .[].tagName' |
    while read -r old; do
      echo "deleting old development pre-release $old (tag kept)"
      gh release delete "$old" --repo "$repo" --yes
    done
fi
