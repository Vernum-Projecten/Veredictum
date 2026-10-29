Every gate over the Arazzo catalogue: the official Arazzo 1.1 schema, source
and parameter resolution, citation resolution against the vendored
specifications, the condition profile, requirement levels, unknown keys, the
rows files, surface coverage, identity and links, and the AQL parser check. It
prints one line per finding and a summary line naming the documents,
workflows, rows files and capability folders it read, and exits `1` if the
finding count is not zero.

Supplying `--statement` adds the static review of that one declaration: it is
held to the published statement schema, every capability it claims must be a
capability folder of the catalogue, and every option arm it selects must be an
arm of an option in `capabilities.yaml`, one arm per option.
