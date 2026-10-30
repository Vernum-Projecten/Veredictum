- The measured record (`results.schema.json`) no longer carries a top-level
  `restapi_specs_version`. The release the server stated is recorded once, in
  `sut.restapi_specs_version` with source `server`, and a document that still
  carries the top-level member fails the published schema.
