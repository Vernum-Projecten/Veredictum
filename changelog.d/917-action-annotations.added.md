- The GitHub Action annotates the run: one error annotation per failed or
  errored row and one warning annotation per warning row, titled with the case
  id and its row and carrying what was checked, the cited sentence with its
  source and section, and what was expected and received. At most 10 of each
  kind are written, GitHub's per-step limit, and a closing notice names how
  many more rows the job summary lists. The `annotations` input turns them
  off. `veredictum export --annotations <file>` writes the same workflow
  commands for any CI that reads them.
