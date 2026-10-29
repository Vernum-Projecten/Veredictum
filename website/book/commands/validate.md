Every gate over the Arazzo catalogue: the official Arazzo 1.1 schema, source
and parameter resolution, citation resolution against the vendored
specifications, the condition profile, requirement levels, unknown keys, the
rows files, surface coverage, identity and links, and the AQL parser check. It
prints one line per finding and a summary line naming the documents,
workflows, rows files and capability folders it read, and exits `1` if the
finding count is not zero.

`capabilities.yaml` also holds every capability folder to two marks. Its
`floors` give each folder the workflow count it keeps: a folder without a
floor, a floor naming no folder, and a folder holding fewer workflows than its
floor are refused (`arazzo-floor`), and the test suite re-derives every floor
from the catalogue, so a change that adds workflows raises the floor with
them. Its `extension-only` list names the folders verified only over the
routes of `catalogue/sources/veredictum-extensions.openapi.yaml`, which no
openEHR OAS describes: a listed folder with a step calling another source,
and an unlisted folder whose every step calls that one, are refused
(`arazzo-realization-scope`). Under `documents`, an `applies` range or a
`formats` entry naming no document is refused, as are a range starting after
its end and a format the document never sends in `Accept` or `Content-Type`.

Supplying `--statement` adds the static review of that one declaration: it is
held to the published statement schema, which requires at least one claimed
capability folder, every capability it claims must be a capability folder of
the catalogue, and every option arm it selects must be an arm of an option in
`capabilities.yaml`, one arm per option. Each claimed folder must then hold a
document the statement lets run: one whose arm it selects, whose `applies`
ranges contain the versions it declares, and whose formats it claims. A claim
nothing can run for is refused (`claim-completeness`), since it would verify
nothing. No openEHR specification governs the floors, the realization scope
or the claim gates; they are this instrument's own design.
