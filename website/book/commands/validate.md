Every gate over the Arazzo catalogue: the official Arazzo 1.1 schema, source
and parameter resolution, citation resolution against the vendored
specifications, the condition profile, requirement levels, unknown keys, rows
files, surface coverage, identity and links, and the AQL parser check. It
prints one line per finding and a summary naming the documents, workflows,
rows files and capability folders read, and exits `1` on any finding.

**`capabilities.yaml` gates.**

- **`floors` (`arazzo-floor`):** each folder's minimum workflow count. Refused:
  a folder without a floor, a floor naming no folder, a folder below its floor.
  The test suite re-derives every floor, so adding workflows raises the floor.
- **`extension-only` (`arazzo-realization-scope`):** the folders verified only
  over Veredictum's own sources (documents under `catalogue/sources/` whose
  `info` carries `x-veredictum-non-openehr: true`, which no openEHR OAS
  describes). Refused: a listed folder with a step calling an unmarked source,
  and an unlisted folder whose every step calls a marked one.
- **`documents`:** refused are an `applies` range or `formats` entry naming no
  document, a range starting after its end, a format the document never sends
  in `Accept` or `Content-Type`, and a document that sends
  `application/json` (itself or through a `dependsOn` setup) without stating
  `canonical-json`.

**Development setups (`arazzo-development-setup`).** A case that rests on
released text and depends, through `dependsOn` at any depth, on a workflow
citing development text alone is refused: the setup would run under the
released profile and the released verdict would rest on development text. A
development-only case may depend on anything.

**`chapters.yaml` (`arazzo-structure`)** is the reading structure every view
follows: it places each capability folder in one chapter and declares each
folder's BASE sections. A document no option arm names is BASE and names its
section with `x-veredictum-section` on its `info`; one an arm names is OPTION
and names none. Refused: a folder in no chapter or two, a BASE document with no
or an unknown section, an unused section, an OPTION document with a section,
an option with no theme and tier, a spec-boundary option whose citation names
no report, and a `reports.yaml` out of step with the reports citations name. A
citation into a document the release marks RETIRED is refused
(`arazzo-citation`).

**`--statement`** adds the static review of one declaration. It must pass the
published statement schema (at least one claimed capability folder), claim
only catalogue folders, and select only arms of options in
`capabilities.yaml`, one per option. Each claimed folder needs a document the
statement lets run: its arm selected, its `applies` ranges containing the
declared versions, its formats claimed. Refused:

- **`claim-completeness`:** a claim nothing can run for.
- **`arm-format`:** a selected arm with a document needing a format the
  statement does not claim, since the run would skip it and nothing would
  verify the arm.

No openEHR specification governs the floors, the realization scope or the
claim gates; they are this instrument's own design.

**`--profile development`** keeps every gate above against the release, then
reads every case citation against the snapshot under
`specs/openehr-development/` beside `--specs`. A citation whose file, section
or sentence the snapshot lacks prints as an `[info]` line naming the case,
followed by per-component counts; neither is a finding, and the exit status
stays `0`. A `profile-citation` finding is a citation the snapshot refuses for
any other reason, a `development` wording the snapshot does not hold, or one
whose released quote still occurs there unchanged. A last line counts the
development-only workflows and the released cases whose cited text the
snapshot rewords or removes. A citation under `specs/openehr-development/`
names development text alone; both profiles check it against the snapshot, and
a bad one is a finding. The repository's own gate
runs `--profile released`, where zero findings is the only pass, and a second
`--profile development` pass that reports the absences.
