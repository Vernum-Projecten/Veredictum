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
routes of Veredictum's own sources, the documents under `catalogue/sources/`
whose `info` carries `x-veredictum-non-openehr: true`, which no openEHR OAS
describes: a listed folder with a step calling an unmarked source, and an
unlisted folder whose every step calls a marked one, are refused
(`arazzo-realization-scope`). Under `documents`, an `applies` range or a
`formats` entry naming no document is refused, as are a range starting after
its end and a format the document never sends in `Accept` or `Content-Type`.
A capability document that sends `application/json`, itself or through a
setup it names in `dependsOn`, and does not state `canonical-json` is refused
too.

`chapters.yaml` is the reading structure every view of a result follows: it
places each capability folder in one chapter and declares each folder's BASE
sections. A document no option arm names is BASE and names its section with
`x-veredictum-section` on its `info`; a document an arm names is OPTION and
names none. A folder in no chapter or in two, a BASE document with no section
or an unknown one, a section no document uses, an OPTION document with a
section, an option with no theme and tier, a spec-boundary option whose
citation names no report, and a `reports.yaml` out of step with the reports
the citations name are refused (`arazzo-structure`). A citation into a
document the release marks RETIRED is refused (`arazzo-citation`).

Supplying `--statement` adds the static review of that one declaration: it is
held to the published statement schema, which requires at least one claimed
capability folder, every capability it claims must be a capability folder of
the catalogue, and every option arm it selects must be an arm of an option in
`capabilities.yaml`, one arm per option. Each claimed folder must then hold a
document the statement lets run: one whose arm it selects, whose `applies`
ranges contain the versions it declares, and whose formats it claims. A claim
nothing can run for is refused (`claim-completeness`), since it would verify
nothing. A selected option arm one of whose documents needs a format the
statement does not claim is refused (`arm-format`): the run would skip that
document, so nothing would verify the arm. No openEHR specification governs the floors, the realization scope
or the claim gates; they are this instrument's own design.

`--profile development` keeps every gate above, against the release, and then
reads every case citation against the development snapshot under
`specs/openehr-development/` beside `--specs`. A citation whose file, section
or sentence the snapshot lacks prints as an `[info]` line naming the case, and
the profile's counts follow per component; neither is a finding, and the exit
status stays `0` for them. A citation the snapshot refuses for any other
reason is a `profile-citation` finding, and so is a citation's `development`
wording that the snapshot does not hold, or one whose released quote still
occurs there unchanged. The repository's own gate runs
`--profile released`, where zero findings is the only passing result, and a
second `--profile development` pass that reports the absences.
