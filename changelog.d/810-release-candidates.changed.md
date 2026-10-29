- A release may be preceded by release candidates, `X.Y.Z-rcN` with N from 1,
  and no other pre-release suffix. `scripts/release/changelog.sh --assemble
  X.Y.Z-rcN YYYY-MM-DD` writes the candidate's section as a release cut would
  and keeps the fragments, so the final release section still carries every
  change, and `--covered <version>` checks that a section carries every
  fragment. The release pipeline runs that check at a candidate tag and refuses
  a tag with any other suffix. A candidate moves no `:latest` and deploys
  nothing to the hosted console.
